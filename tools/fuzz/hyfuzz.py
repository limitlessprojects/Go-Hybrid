#!/usr/bin/env python3
"""hyfuzz.py — the parser/checker fuzz leg (post-MVP row 2).

Coverage-guided fuzzing needs nightly Rust (cargo-fuzz/libFuzzer); the toolchain
is PINNED stable (SPEC §2), so this leg delivers the same crash-class invariants
through deterministic seeded generation + corpus mutation instead, honestly
documented as such in the job note:

  INVARIANT 1 (no hang):    `hyb check` on ANY input terminates within the
                            per-case timeout (the 2026-10-14 sync_stmt/
                            sync_decl force-progress fix is what this pins).
  INVARIANT 2 (no crash):   exit code is 0 (accept) or 1 (clean reject);
                            any other code, including negative POSIX signal
                            codes, fails the case.
  INVARIANT 3 (no ICE):     stdout/stderr never contains "panicked at" or
                            "internal compiler error".
  INVARIANT 4 (Go oracle):  when --oracle-build is enabled, if `hyb check`
                            accepts (exit 0), the emitted Go must compile
                            cleanly under `hyb build` (`go build`).

Strategies (deterministic seed → reproducible job):
  1. token-salad   — random sequences drawn from the language's token alphabet
  2. corpus-mutate — splice/delete/duplicate/replace tokens in the
                     pre-registered corpus programs (they are the spec)
  3. nest-bomb     — deep nesting of the brace/paren shapes the sync loops
                     walk (regression pressure on the recovery path)
  4. go-decidable  — seeded grammar generator restricted to Go-decidable rule
                     families (widths/ranges, declarations/buildability,
                     indexing/bounds, equality/comparability, shifts/len,
                     call/return arity) for measuring residual accept-side
                     oracle disagreement per rule family (#565 M1/M2).

Stdlib only (runner discipline). Exit 0 iff all invariants held for all cases.

Optional: --failures-dir PATH preserves complete failing inputs and seed/case
metadata beneath a unique run directory. Successful runs create no artifacts;
without the option, the existing console-only behavior is unchanged.
"""
import argparse
import hashlib
import json
import math
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

TOKENS = [
    "fn", "main", "(", ")", "{", "}", "[", "]", "let", "var", "const", "return",
    "if", "else", "for", "in", "loop", "break", "continue", "select", "default",
    "match", "task", "defer", "go", "chan", "make", "len", "cap", "append",
    "struct", "enum", "import", "move", "managed", "panic", "try", "nil",
    "int", "string", "bool", "float64", "map[string]int", "chan int", "*T",
    ":=", "=", "+=", "<-", "==", "&&", "||", "!", "+", "-", "*", "/", "%",
    "<", ">", ";", ",", ".", ":", "?", "&", "\"str\"", "`raw`", "\"interp{x}\"",
    "x", "y", "ch", "counts", "words", "f", "0", "1", "3.14", "_", "v", "ok",
    "http", "fmt", "strings", "atomic", "serde_json", "//comment", "\n",
]

NEST_SHAPES = [
    "fn main() {{ {body} }}",
    "select {{ {body} }}",
    "for i in 0..3 {{ {body} }}",
    "if x {{ {body} }} else {{ {body2} }}",
    "match x {{ 0: {{ {body} }} }}",
    "managed {{ {body} }}",
]

BAD_TEXT = ("panicked at", "internal compiler error", "stack overflow")

GO_DECIDABLE_FAMILIES = (
    "const_overflow",
    "buildable_decl",
    "index_and_bounds",
    "equality_compare",
    "shift_and_len",
    "call_and_return",
)

# Negative controls that compile cleanly in Go and MUST NOT be rejected as
# constant-overflow or compile-time out-of-bounds (#577 / #576):
#   1. Slice runtime out-of-bounds (panics at runtime in Go, not a compile error)
#   2. Runtime unary negation of int8::MIN (wraps at runtime in Go, not a constant)
#   3. Fixed-array length 300 with int8 element type (length is typed `int`, not `int8`)
ORACLE_NEGATIVE_CONTROLS = (
    (
        "slice_runtime_oob",
        'fn main() {\n    xs := []int{1, 2}\n    fmt.println("{xs[300]}")\n}\n',
    ),
    (
        "runtime_unary_neg_min_int8",
        'fn main() {\n    var n int8 = -128\n    var m int8 = -n\n    fmt.println("{m}")\n}\n',
    ),
    (
        "array_len_300_int8",
        'fn main() {\n    var a [300]int8\n    fmt.println("{len(a)}")\n}\n',
    ),
)

VERDICT_RE = re.compile(r"^//[ \t]*EXPECTED-VERDICT:[ \t]*([^\r\n]*)", re.M)


def gen_salad(rng, n):
    return " ".join(rng.choice(TOKENS) for _ in range(rng.randint(1, n)))


def load_corpus(corpus_dir):
    progs = []
    for f in sorted(os.listdir(corpus_dir)):
        if f.endswith(".hy"):
            with open(os.path.join(corpus_dir, f), encoding="utf-8") as fh:
                progs.append(fh.read())
    return progs


def gen_mutate(rng, corpus):
    src = list(rng.choice(corpus))
    for _ in range(rng.randint(1, 12)):
        if not src:
            break
        op = rng.random()
        i = rng.randrange(len(src))
        if op < 0.35:  # delete
            del src[i:i + rng.randint(1, 8)]
        elif op < 0.6:  # duplicate a chunk
            j = min(len(src), i + rng.randint(1, 12))
            src[i:i] = src[i:j]
        elif op < 0.8:  # splice a random token
            src[i:i] = rng.choice(TOKENS)
        else:  # swap two chunks
            j = rng.randrange(len(src))
            src[i], src[j] = src[j], src[i]
    return "".join(src)


def gen_nest(rng):
    body = 'fmt.println("deep")'
    for _ in range(rng.randint(3, 30)):
        shape = rng.choice(NEST_SHAPES)
        if "{body2}" in shape:
            # Keep one recursively deep path. Duplicating it in both arms
            # doubles the source at each branch, before the checker timeout
            # even starts; 30 branch levels would produce 2**30 leaves.
            body = shape.format(body=body, body2='fmt.println("shallow")')
        else:
            body = shape.format(body=body)
    return body


def gen_go_decidable(rng, family=None):
    """Generate (rule_family, src) from the pre-registered Go-decidable grammar.

    Covers six Go-decidable rule domains (#565 M2):
      - const_overflow   (HYB-E6002: integer widths & constant ranges)
      - buildable_decl   (HYB-E1002 / BUILDABLE: local declarations & usage)
      - index_and_bounds (HYB-E6004 / HYB-E6001: indexing, slicing, div/mod)
      - equality_compare (HYB-E4009: comparable vs incomparable == / !=)
      - shift_and_len    (HYB-E6005 / HYB-E6006: shifts & len/cap arguments)
      - call_and_return  (HYB-E4004 / HYB-E4005 / HYB-E4006: call/return arity)
    """
    if family is None:
        family = rng.choice(GO_DECIDABLE_FAMILIES)

    if family == "const_overflow":
        ty = rng.choice(("int8", "uint8", "int16"))
        lit = rng.choice(("42", "-5", "300", "70000"))
        shape = rng.choice((
            "var_init",
            "direct_assign",
            "alias_assign",
            "named_const_assign",
            "binary_expr",
            "map_key_overflow",
            "untyped_shift_overflow",
            "runtime_unary_neg_ok",
            "array_len_300_int8_ok",
        ))
        if shape == "var_init":
            src = f"fn main() {{\n    var x {ty} = {lit}\n    _ = x\n}}\n"
        elif shape == "direct_assign":
            src = f"fn main() {{\n    var x {ty} = 1\n    x = {lit}\n    _ = x\n}}\n"
        elif shape == "alias_assign":
            src = (
                f"type narrow = {ty}\n"
                f"fn main() {{\n    var x narrow = 1\n    x = {lit}\n    _ = x\n}}\n"
            )
        elif shape == "named_const_assign":
            src = (
                f"const k = {lit}\n"
                f"fn main() {{\n    var x {ty} = 1\n    x = k\n    _ = x\n}}\n"
            )
        elif shape == "binary_expr":
            op = rng.choice(("+", "-"))
            src = (
                f"fn main() {{\n"
                f"    var x {ty} = 1\n"
                f"    y := x {op} {lit}\n"
                f"    _ = y\n"
                f"}}\n"
            )
        elif shape == "map_key_overflow":
            src = f"fn main() {{\n    m := map[{ty}]int{{}}\n    m[{lit}] = 1\n    _ = m\n}}\n"
        elif shape == "untyped_shift_overflow":
            src = 'fn main() {\n    n := 1 << 200\n    fmt.println("{n}")\n}\n'
        elif shape == "runtime_unary_neg_ok":
            src = 'fn main() {\n    var n int8 = -128\n    var m int8 = -n\n    fmt.println("{m}")\n}\n'
        else:
            src = 'fn main() {\n    var a [300]int8\n    fmt.println("{len(a)}")\n}\n'
        return family, src

    if family == "buildable_decl":
        decl_kind = rng.choice((
            "short_used",
            "short_unused",
            "let_used",
            "let_unused",
            "var_used",
            "var_unused",
            "var_untyped_unused",
        ))
        val = rng.choice(("1", "42", '"ok"', "true"))
        ty = "int" if val in ("1", "42") else ("string" if val == '"ok"' else "bool")
        if decl_kind == "short_used":
            src = f"fn main() {{\n    x := {val}\n    _ = x\n}}\n"
        elif decl_kind == "short_unused":
            src = f"fn main() {{\n    x := {val}\n}}\n"
        elif decl_kind == "let_used":
            src = f"fn main() {{\n    let x = {val}\n    _ = x\n}}\n"
        elif decl_kind == "let_unused":
            src = f"fn main() {{\n    let x = {val}\n}}\n"
        elif decl_kind == "var_used":
            src = f"fn main() {{\n    var x {ty} = {val}\n    _ = x\n}}\n"
        elif decl_kind == "var_unused":
            src = f"fn main() {{\n    var x {ty} = {val}\n}}\n"
        else:
            src = f"fn main() {{\n    var x = {val}\n}}\n"
        return family, src

    if family == "index_and_bounds":
        shape = rng.choice((
            "slice_pos_index",
            "slice_neg_index",
            "slice_neg_low_bound",
            "slice_runtime_oob_ok",
            "array_const_oob",
            "div_nonzero",
            "div_zero",
            "mod_zero",
        ))
        if shape == "slice_pos_index":
            idx = rng.choice(("0", "1"))
            src = f"fn main() {{\n    s := []int{{10, 20}}\n    v := s[{idx}]\n    _ = v\n}}\n"
        elif shape == "slice_neg_index":
            idx = rng.choice(("-1", "-2"))
            src = f"fn main() {{\n    s := []int{{10, 20}}\n    v := s[{idx}]\n    _ = v\n}}\n"
        elif shape == "slice_neg_low_bound":
            src = "fn main() {\n    s := []int{10, 20}\n    v := s[-1:1]\n    _ = v\n}\n"
        elif shape == "slice_runtime_oob_ok":
            src = 'fn main() {\n    xs := []int{1, 2}\n    fmt.println("{xs[300]}")\n}\n'
        elif shape == "array_const_oob":
            src = 'fn main() {\n    var a [2]int\n    a[300] = 1\n    fmt.println("{a[0]}")\n}\n'
        elif shape == "div_nonzero":
            d = rng.choice(("2", "5"))
            src = f"fn main() {{\n    x := 100 / {d}\n    _ = x\n}}\n"
        elif shape == "div_zero":
            src = "fn main() {\n    x := 100 / 0\n    _ = x\n}\n"
        else:
            src = "fn main() {\n    x := 100 % 0\n    _ = x\n}\n"
        return family, src

    if family == "equality_compare":
        op = rng.choice(("==", "!="))
        shape = rng.choice((
            "int_eq",
            "string_eq",
            "struct_scalar_eq",
            "slice_eq",
            "map_eq",
            "struct_with_slice_eq",
        ))
        if shape == "int_eq":
            src = f"fn main() {{\n    a := 1\n    b := 2\n    c := a {op} b\n    _ = c\n}}\n"
        elif shape == "string_eq":
            src = f'fn main() {{\n    a := "x"\n    b := "y"\n    c := a {op} b\n    _ = c\n}}\n'
        elif shape == "struct_scalar_eq":
            src = (
                f"struct pair {{\n    a int\n    b int\n}}\n"
                f"fn main() {{\n    p1 := pair{{a: 1, b: 2}}\n    p2 := pair{{a: 1, b: 3}}\n"
                f"    c := p1 {op} p2\n    _ = c\n}}\n"
            )
        elif shape == "slice_eq":
            src = f"fn main() {{\n    a := []int{{1}}\n    b := []int{{1}}\n    c := a {op} b\n    _ = c\n}}\n"
        elif shape == "map_eq":
            src = (
                f"fn main() {{\n    a := make(map[string]int)\n    b := make(map[string]int)\n"
                f"    c := a {op} b\n    _ = c\n}}\n"
            )
        else:
            src = (
                f"struct bag {{\n    items []int\n}}\n"
                f"fn main() {{\n    b1 := bag{{items: []int{{1}}}}\n    b2 := bag{{items: []int{{1}}}}\n"
                f"    c := b1 {op} b2\n    _ = c\n}}\n"
            )
        return family, src

    if family == "shift_and_len":
        shape = rng.choice((
            "shift_pos",
            "shift_neg",
            "shift_float",
            "len_slice",
            "len_string",
            "len_int",
        ))
        if shape == "shift_pos":
            amt = rng.choice(("1", "4", "8"))
            src = f"fn main() {{\n    x := 1 << {amt}\n    _ = x\n}}\n"
        elif shape == "shift_neg":
            src = "fn main() {\n    x := 1 << -1\n    _ = x\n}\n"
        elif shape == "shift_float":
            src = "fn main() {\n    x := 1.5 << 2\n    _ = x\n}\n"
        elif shape == "len_slice":
            src = "fn main() {\n    s := []int{1, 2, 3}\n    n := len(s)\n    _ = n\n}\n"
        elif shape == "len_string":
            src = 'fn main() {\n    n := len("hybrid")\n    _ = n\n}\n'
        else:
            src = "fn main() {\n    x := 10\n    n := len(x)\n    _ = n\n}\n"
        return family, src

    # call_and_return
    shape = rng.choice((
        "arity_ok",
        "arity_too_few",
        "arity_too_many",
        "return_ok",
        "return_wrong_type",
        "multi_bind_ok",
    ))
    if shape == "arity_ok":
        src = "fn add(a int, b int) int {\n    return a + b\n}\nfn main() {\n    v := add(1, 2)\n    _ = v\n}\n"
    elif shape == "arity_too_few":
        src = "fn add(a int, b int) int {\n    return a + b\n}\nfn main() {\n    v := add(1)\n    _ = v\n}\n"
    elif shape == "arity_too_many":
        src = "fn add(a int, b int) int {\n    return a + b\n}\nfn main() {\n    v := add(1, 2, 3)\n    _ = v\n}\n"
    elif shape == "return_ok":
        src = "fn id(x int) int {\n    return x\n}\nfn main() {\n    v := id(7)\n    _ = v\n}\n"
    elif shape == "return_wrong_type":
        src = 'fn bad() int {\n    return "not-an-int"\n}\nfn main() {\n    v := bad()\n    _ = v\n}\n'
    else:
        src = (
            "fn pair(x int) (int, int) {\n    return x, x + 1\n}\n"
            "fn main() {\n    a, b := pair(5)\n    _ = a\n    _ = b\n}\n"
        )
    return family, src


def check_one(hyb, src, workdir, timeout):
    """Return (failure, accepted) from one checker execution."""
    path = os.path.join(workdir, "case.hy")
    with open(path, "w", encoding="utf-8") as f:
        f.write(src)
    try:
        r = subprocess.run(
            [hyb, "check", path], cwd=workdir, capture_output=True,
            text=True, timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return "HANG", False
    except OSError as exc:
        if getattr(exc, "winerror", None) == 193:
            try:
                r = subprocess.run(
                    [sys.executable, hyb, "check", path], cwd=workdir,
                    capture_output=True, text=True, timeout=timeout,
                )
            except subprocess.TimeoutExpired:
                return "HANG", False
            except OSError as err:
                return f"EXEC_ERROR: {err}", False
        else:
            return f"EXEC_ERROR: {exc}", False
    if r.returncode not in (0, 1):
        return f"UNEXPECTED exit={r.returncode}", False
    blob = (r.stdout + r.stderr).lower()
    if any(b in blob for b in BAD_TEXT):
        return f"ICE: {(r.stdout + r.stderr)[:120]}", False
    return None, r.returncode == 0


def _first_go_compiler_line(text):
    """Extract the first informative compiler diagnostic line from go build output."""
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or line == "go build failed:":
            continue
        return line[:200]
    return text.strip()[:200]


def _oracle_nocgo_fallback(hyb, path, workdir, timeout):
    """Verify Go type-checking with CGO_ENABLED=0 when host lacks gcc for cgo bindings."""
    rt_go = os.environ.get("HYB_RUNTIME_DIR")
    if rt_go:
        rt_go = os.path.join(rt_go, "go")
    else:
        rt_go = os.path.join(REPO_ROOT, "runtime", "go")
    if not os.path.isdir(rt_go):
        return "ORACLE_TOOL_ERROR: cgo C compiler missing and runtime/go not found", False

    mod_dir = tempfile.mkdtemp(prefix="hyoracle-nocgo-", dir=workdir)
    try:
        out_base = os.path.join(mod_dir, "main")
        emit = subprocess.run(
            [hyb, "build", path, "-o", out_base, "--emit-go"],
            cwd=mod_dir, capture_output=True, text=True, timeout=timeout,
        )
        if emit.returncode != 0:
            detail = _first_go_compiler_line(emit.stderr or emit.stdout)
            return f"ORACLE_EMIT_FAIL: {detail}", False
        with open(os.path.join(mod_dir, "go.mod"), "w", encoding="utf-8") as fh:
            fh.write("module hybrid.program\n\ngo 1.26\n")
        for pkg in ("hybridge", "jsonbinding", "sha3binding", "regexbinding"):
            src_pkg = os.path.join(rt_go, pkg)
            dst_pkg = os.path.join(mod_dir, pkg)
            if os.path.isdir(src_pkg):
                shutil.copytree(src_pkg, dst_pkg)
                broker = os.path.join(dst_pkg, "broker.go")
                if os.path.isfile(broker):
                    with open(broker, encoding="utf-8") as bf:
                        btxt = bf.read()
                    btxt = btxt.replace('import "C"\n', "")
                    btxt = re.sub(r"C\.[A-Za-z0-9_]+\(\)", "uintptr(0)", btxt)
                    with open(broker, "w", encoding="utf-8") as bf:
                        bf.write(btxt)
        env = os.environ.copy()
        env["CGO_ENABLED"] = "0"
        out_exe = os.path.join(mod_dir, "app.exe" if os.name == "nt" else "app")
        build = subprocess.run(
            ["go", "build", "-o", out_exe, "."],
            cwd=mod_dir, env=env, capture_output=True, text=True, timeout=timeout,
        )
        if build.returncode == 0:
            return None, True
        detail = _first_go_compiler_line(build.stderr or build.stdout)
        return f"ORACLE_BUILD_FAIL: {detail}", False
    except subprocess.TimeoutExpired:
        return "ORACLE_HANG", False
    finally:
        shutil.rmtree(mod_dir, ignore_errors=True)


def oracle_one(hyb, src, workdir, timeout):
    """INVARIANT 4 (#565): an accepted .hy program must compile under `hyb build`.

    Returns (failure, built) where failure is None iff the emitted Go built cleanly.
    """
    path = os.path.join(workdir, "case.hy")
    out_bin = os.path.join(workdir, "oracle_out.exe" if os.name == "nt" else "oracle_out")
    with open(path, "w", encoding="utf-8") as f:
        f.write(src)
    if os.path.exists(out_bin):
        try:
            os.remove(out_bin)
        except OSError:
            pass
    try:
        r = subprocess.run(
            [hyb, "build", path, "-o", out_bin],
            cwd=workdir, capture_output=True, text=True, timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return "ORACLE_HANG", False
    except OSError as exc:
        if getattr(exc, "winerror", None) == 193:
            try:
                r = subprocess.run(
                    [sys.executable, hyb, "build", path, "-o", out_bin],
                    cwd=workdir, capture_output=True, text=True, timeout=timeout,
                )
            except subprocess.TimeoutExpired:
                return "ORACLE_HANG", False
            except OSError as err:
                return f"ORACLE_EXEC_ERROR: {err}", False
        else:
            return f"ORACLE_EXEC_ERROR: {exc}", False
    combined = (r.stdout or "") + (r.stderr or "")
    blob = combined.lower()
    if any(b in blob for b in BAD_TEXT):
        return f"ORACLE_ICE: {combined.strip()[:120]}", False
    if r.returncode == 0:
        return None, True
    if 'cgo: c compiler' in blob and "not found" in blob:
        return _oracle_nocgo_fallback(hyb, path, workdir, timeout)
    if r.returncode not in (0, 1):
        detail = _first_go_compiler_line(r.stderr or r.stdout)
        return f"ORACLE_TOOL_ERROR exit={r.returncode}: {detail}", False
    detail = _first_go_compiler_line(r.stderr or r.stdout)
    return f"ORACLE_BUILD_FAIL: {detail}", False


def audit_corpus_oracle(hyb, corpus_dir, timeout=30.0):
    """M1 (#565): measure accept-side Go toolchain disagreement across a corpus."""
    files = sorted(
        os.path.join(corpus_dir, f)
        for f in os.listdir(corpus_dir)
        if f.endswith(".hy")
    )
    abs_corpus = os.path.abspath(corpus_dir)
    try:
        rel_corpus = os.path.relpath(abs_corpus, REPO_ROOT).replace(os.sep, "/")
    except ValueError:
        rel_corpus = abs_corpus
    report = {
        "corpus_dir": rel_corpus,
        "corpus_total": 0,
        "accept_total": 0,
        "accept_built": 0,
        "accept_disagreements": [],
        "reject_total": 0,
        "reject_rejected": 0,
        "verdict_mismatches": [],
        "accept_disagreement_rate": 0.0,
    }
    with tempfile.TemporaryDirectory(prefix="hyfuzz-audit-") as workdir:
        for path in files:
            with open(path, encoding="utf-8") as fh:
                src = fh.read()
            markers = VERDICT_RE.findall(src)
            expected = markers[0].split()[0] if markers else None
            report["corpus_total"] += 1
            name = os.path.basename(path)
            bad, accepted = check_one(hyb, src, workdir, timeout)
            if bad is not None:
                report["verdict_mismatches"].append({"file": name, "error": bad})
                continue
            if expected == "REJECT":
                report["reject_total"] += 1
                if not accepted:
                    report["reject_rejected"] += 1
                else:
                    report["verdict_mismatches"].append(
                        {"file": name, "error": "checker accepted expected REJECT"}
                    )
            else:
                report["accept_total"] += 1
                if not accepted:
                    report["verdict_mismatches"].append(
                        {"file": name, "error": "checker rejected expected ACCEPT"}
                    )
                    continue
                obad, built = oracle_one(hyb, src, workdir, timeout)
                if obad is None and built:
                    report["accept_built"] += 1
                else:
                    report["accept_disagreements"].append(
                        {"file": name, "failure": obad or "build failed"}
                    )
    if report["accept_total"] > 0:
        report["accept_disagreement_rate"] = round(
            len(report["accept_disagreements"]) / report["accept_total"], 6
        )
    return report


def save_failure(run_dir, seed, index, kind, failure, src):
    """Write data-only reproductions using generated names and exclusive creates."""
    filename = f"case-{index:06d}.hy"
    source_path = os.path.join(run_dir, filename)
    encoded = src.encode("utf-8")
    with open(source_path, "xb") as f:
        f.write(encoded)
    metadata = {
        "version": 1,
        "seed": seed,
        "case_index": index,
        "kind": kind,
        "failure": failure,
        "source_file": filename,
        "source_sha256": hashlib.sha256(encoded).hexdigest(),
    }
    with open(os.path.join(run_dir, f"case-{index:06d}.json"), "x", encoding="utf-8") as f:
        json.dump(metadata, f, indent=2, sort_keys=True)
        f.write("\n")
    return source_path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--hyb", required=True)
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--cases", type=int, default=1200)
    ap.add_argument("--seed", type=int, default=20261014)
    ap.add_argument("--timeout", type=float, default=10.0)
    ap.add_argument(
        "--failures-dir", default=None, metavar="PATH",
        help="save full failing inputs and metadata in a unique run directory under PATH",
    )
    ap.add_argument(
        "--strategy", choices=("default", "go-decidable", "all"), default="default",
        help="generation strategy: default (token-salad/corpus-mutate/nest-bomb), "
             "go-decidable (seeded Go-decidable grammar, #565 M2), or all",
    )
    ap.add_argument(
        "--oracle-build", action="store_true", default=False,
        help="INVARIANT 4 (#565): verify every checker-accepted case builds via `hyb build`",
    )
    ap.add_argument(
        "--audit-corpus", action="store_true", default=False,
        help="M1 (#565): measure accept-side Go toolchain disagreement on --corpus",
    )
    ap.add_argument(
        "--oracle-report", default=None, metavar="PATH",
        help="write JSON summary of M1 corpus audit and M2 per-rule oracle residuals to PATH",
    )
    ap.add_argument(
        "--max-failures", type=int, default=10,
        help="stop after N invariant violations (0 = run all cases)",
    )
    args = ap.parse_args()

    rng = random.Random(args.seed)
    # resolve BEFORE any subprocess: check_one runs with cwd=workdir (a temp
    # dir), so a relative --hyb path would resolve THERE (the 0.1s fleet
    # failure of 2026-10-14 — FileNotFoundError, exit 1 before case 1)
    args.hyb = os.path.abspath(args.hyb)
    if not os.path.isfile(args.hyb):
        raise SystemExit(f"hyfuzz: --hyb binary not found: {args.hyb}")
    if not os.path.isdir(args.corpus):
        raise SystemExit(f"hyfuzz: --corpus directory not found: {args.corpus}")
    if args.cases <= 0:
        raise SystemExit("hyfuzz: --cases must be positive")
    if not (math.isfinite(args.timeout) and args.timeout > 0):
        raise SystemExit("hyfuzz: --timeout must be positive")
    corpus = load_corpus(args.corpus)
    if not corpus:
        raise SystemExit("fuzz: no corpus programs found")

    corpus_audit = None
    if args.audit_corpus:
        corpus_audit = audit_corpus_oracle(args.hyb, args.corpus, max(args.timeout, 30.0))
        print(
            f"hyfuzz: corpus-audit total={corpus_audit['corpus_total']} "
            f"accept={corpus_audit['accept_built']}/{corpus_audit['accept_total']} "
            f"reject={corpus_audit['reject_rejected']}/{corpus_audit['reject_total']} "
            f"accept-disagreement-rate={corpus_audit['accept_disagreement_rate']:.4f}"
        )

    failures = []
    failure_run_dir = None
    clean_rejects = ok_accepts = 0
    oracle_checked = oracle_disagreements = 0
    per_family = {
        fam: {"generated": 0, "checker_rejected": 0, "checker_accepted": 0, "oracle_failed": 0}
        for fam in GO_DECIDABLE_FAMILIES
    }

    with tempfile.TemporaryDirectory(prefix="hyfuzz-") as workdir:
        for i in range(args.cases):
            family = None
            if args.strategy == "go-decidable":
                family = GO_DECIDABLE_FAMILIES[i % len(GO_DECIDABLE_FAMILIES)]
                kind = family
                _, src = gen_go_decidable(rng, family)
                per_family[family]["generated"] += 1
            elif args.strategy == "all":
                slot = i % 4
                if slot == 0:
                    kind = 0
                    src = gen_salad(rng, 60)
                elif slot == 1:
                    kind = 1
                    src = gen_mutate(rng, corpus)
                elif slot == 2:
                    kind = 2
                    src = gen_nest(rng)
                else:
                    family = GO_DECIDABLE_FAMILIES[(i // 4) % len(GO_DECIDABLE_FAMILIES)]
                    kind = family
                    _, src = gen_go_decidable(rng, family)
                    per_family[family]["generated"] += 1
            else:
                kind = i % 3
                if kind == 0:
                    src = gen_salad(rng, 60)
                elif kind == 1:
                    src = gen_mutate(rng, corpus)
                else:
                    src = gen_nest(rng)

            bad, accepted = check_one(args.hyb, src, workdir, args.timeout)
            if bad is None and accepted and args.oracle_build:
                oracle_checked += 1
                obad, _ = oracle_one(args.hyb, src, workdir, args.timeout)
                if obad is not None:
                    bad = obad
                    oracle_disagreements += 1
                    if family is not None:
                        per_family[family]["checker_accepted"] += 1
                        per_family[family]["oracle_failed"] += 1

            if bad:
                failures.append((i, kind, bad, src[:200]))
                if args.failures_dir is not None:
                    try:
                        if failure_run_dir is None:
                            os.makedirs(args.failures_dir, exist_ok=True)
                            failure_run_dir = tempfile.mkdtemp(
                                prefix="hyfuzz-", dir=os.path.abspath(args.failures_dir),
                            )
                        saved = save_failure(failure_run_dir, args.seed, i, kind, bad, src)
                    except OSError as exc:
                        print(f"hyfuzz: cannot save failure artifact: {exc}", file=sys.stderr)
                        raise SystemExit(2) from exc
                    print(f"hyfuzz: reproducer saved: {saved}")
                if args.max_failures > 0 and len(failures) >= args.max_failures:
                    break
            elif accepted:
                ok_accepts += 1
                if family is not None:
                    per_family[family]["checker_accepted"] += 1
            else:
                clean_rejects += 1
                if family is not None:
                    per_family[family]["checker_rejected"] += 1

    if args.oracle_report is not None:
        for fam, stats in per_family.items():
            acc = stats["checker_accepted"]
            stats["oracle_residual_rate"] = round(stats["oracle_failed"] / acc, 6) if acc else 0.0
        report_payload = {
            "seed": args.seed,
            "strategy": args.strategy,
            "cases": args.cases,
            "clean_rejects": clean_rejects,
            "clean_accepts": ok_accepts,
            "oracle_build_enabled": args.oracle_build,
            "oracle_checked": oracle_checked,
            "oracle_disagreements": oracle_disagreements,
            "per_family": per_family,
            "corpus_audit": corpus_audit,
            "failures": [
                {"case_index": i, "kind": k, "failure": b, "source_head": h}
                for i, k, b, h in failures
            ],
        }
        report_path = os.path.abspath(args.oracle_report)
        os.makedirs(os.path.dirname(report_path) or ".", exist_ok=True)
        with open(report_path, "w", encoding="utf-8") as rf:
            json.dump(report_payload, rf, indent=2, sort_keys=True)
            rf.write("\n")

    print(f"hyfuzz: {args.cases} cases, seed={args.seed}, "
          f"clean-rejects={clean_rejects}, clean-accepts={ok_accepts}")
    if corpus_audit is not None and (
        corpus_audit["accept_disagreements"] or corpus_audit["verdict_mismatches"]
    ):
        print(
            f"hyfuzz: corpus audit found "
            f"{len(corpus_audit['accept_disagreements'])} accept-side disagreement(s) and "
            f"{len(corpus_audit['verdict_mismatches'])} verdict mismatch(es)"
        )
        raise SystemExit(1)
    if failures:
        print(f"hyfuzz: {len(failures)} INVARIANT VIOLATIONS:")
        for i, kind, bad, head in failures:
            print(f"  case {i} (kind {kind}): {bad}\n    {head!r}")
        raise SystemExit(1)
    print("hyfuzz: all invariants held (no hangs, no crashes, no ICEs)")


if __name__ == "__main__":
    main()
