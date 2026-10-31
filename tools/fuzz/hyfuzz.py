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

Strategies (deterministic seed → reproducible job):
  1. token-salad   — random sequences drawn from the language's token alphabet
  2. corpus-mutate — splice/delete/duplicate/replace tokens in the 20
                     pre-registered corpus programs (they are the spec)
  3. nest-bomb     — deep nesting of the brace/paren shapes the sync loops
                     walk (regression pressure on the recovery path)

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
import subprocess
import sys
import tempfile

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


def check_one(hyb, src, workdir, timeout):
    """Return (failure, accepted) from one checker execution."""
    path = os.path.join(workdir, "case.hy")
    # Keep the checked input identical to save_failure's bytes and hash,
    # including on Windows where a text-mode write would expand newlines.
    with open(path, "wb") as f:
        f.write(src.encode("utf-8"))
    try:
        r = subprocess.run(
            [hyb, "check", path], cwd=workdir, capture_output=True,
            text=True, timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return "HANG", False
    except OSError as err:
        return f"EXEC_ERROR: {err}", False
    if r.returncode not in (0, 1):
        return f"UNEXPECTED exit={r.returncode}", False
    blob = (r.stdout + r.stderr).lower()
    if any(b in blob for b in BAD_TEXT):
        return f"ICE: {(r.stdout + r.stderr)[:120]}", False
    return None, r.returncode == 0


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

    failures = []
    failure_run_dir = None
    clean_rejects = ok_accepts = 0
    with tempfile.TemporaryDirectory(prefix="hyfuzz-") as workdir:
        for i in range(args.cases):
            kind = i % 3
            if kind == 0:
                src = gen_salad(rng, 60)
            elif kind == 1:
                src = gen_mutate(rng, corpus)
            else:
                src = gen_nest(rng)
            bad, accepted = check_one(args.hyb, src, workdir, args.timeout)
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
                if len(failures) >= 10:
                    break
            elif accepted:
                ok_accepts += 1
            else:
                clean_rejects += 1

    print(f"hyfuzz: {args.cases} cases, seed={args.seed}, "
          f"clean-rejects={clean_rejects}, clean-accepts={ok_accepts}")
    if failures:
        print(f"hyfuzz: {len(failures)} INVARIANT VIOLATIONS:")
        for i, kind, bad, head in failures:
            print(f"  case {i} (kind {kind}): {bad}\n    {head!r}")
        raise SystemExit(1)
    print("hyfuzz: all invariants held (no hangs, no crashes, no ICEs)")


if __name__ == "__main__":
    main()
