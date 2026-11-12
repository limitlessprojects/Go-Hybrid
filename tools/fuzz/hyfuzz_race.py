#!/usr/bin/env python3
"""hyfuzz_race.py — mutation fuzzing of the ACCEPT corpus with a race oracle.

hyfuzz.py pins the crash-class invariants (no hang, no crash, no ICE) on ANY
input. This companion leg pins the soundness invariant on the *accepted* side:
an accepted program is a proof that the program races never — so a mutant the
checker still accepts that does race under Go's race detector falsifies the
proof, and is a P0 candidate by definition (SPEC §10, the soundness template).

  INVARIANT (accepted programs never race):
    a mutated corpus program that `hyb check` accepts, that survives
    `hyb build --emit-go` + `go build -race`, and whose run report
    "WARNING: DATA RACE" (or a Go concurrent-map fault) is a soundness
    candidate — the oracle prints it, saves a data-only reproducer, and
    exits non-zero.

Mutation strategies (deterministic seed → reproducible job), applied to the
`fn main` body only so helper functions and the general program shape survive:

  1. perm    — a cyclic rotation of the main-body statements: every
               use-after-decl order stays legal for Go bindings while the
               order of freeze/alias/move transitions is reshuffled.
  2. dup     — duplicate a single declaration-free statement.
  3. splice  — inject one concurrency activation (task worker over an
               existing mutable-GC binding, move/plain channel send, or a
               pointer loan) with its wiring predeclared.

Note reachable-but-not-instrumented: programs whose emit crosses the rust
bridge (`import` of a bridge package) cannot be built by a flat single-file
`go build`; those cases are reported as `skip-bridge`, not as failures.

Race detection is necessarily probablistic: an accepted-but-racing program
may not trip the detector on a single run. The job is meant to be re-run with
different --seed values; each run is fully reproducible from its seed.
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
from concurrent.futures import ThreadPoolExecutor

MAIN_RE = re.compile(r"^fn main\(\) \{\n(.*?)^\}\n?$", re.M | re.S)
ACCEPTED_RE = re.compile(r"^//\s*EXPECTED-VERDICT:\s*(\S+)", re.M)
BAD_TEXT = ("panicked at", "internal compiler error", "stack overflow")

# Helpers only used by the splice strategy; every name is standalone, so the
# mutants stay single-file `hyb build --emit-go` compatible.
HELPERS = """\
fn hz_wkr(m map[string]int, kill <-chan bool) {
    for !(<-kill) {
        m["hzz"] = 1
        for i in 0..2000000 { }
    }
}

fn hz_recvr(c <-chan map[string]int, done chan<- bool) {
    g := <-c
    g["hzzr"] = 1
    done <- true
}
"""

# {u} is bound to an existing mutable-GC binding name found in the body;
# wiring (k_hz / chz) is predeclared around the injected statement.
SPLICE_TPL = [
    "task hz_wkr({u}, k_hz)",
    "chz <- move {u}",
    "chz <- {u}",
    'p_hz := &{u}["hzz"]\n    _ = p_hz',
]


def load_accepts(corpus_dir):
    """Corpus programs whose verdict is any ACCEPT variant (they are the
    proven-sendable population mutations are drawn from)."""
    progs = []
    for f in sorted(os.listdir(corpus_dir)):
        if not f.endswith(".hy"):
            continue
        with open(os.path.join(corpus_dir, f), encoding="utf-8") as fh:
            src = fh.read()
        m = ACCEPTED_RE.search(src)
        if m and m.group(1).startswith("ACCEPT"):
            progs.append(src)
    return progs


def split_main_body(src):
    """Split `fn main`'s body into statements: a line starting with exactly
    four spaces opens a body statement, deeper-indented / blank lines ride
    along with the statement they belong to (the layout corpus programs
    actually use)."""
    m = MAIN_RE.search(src)
    if not m:
        return None
    stmts, cur = [], []

    def flush():
        while cur and cur[-1].strip() == "":
            cur.pop()
        if cur:
            stmts.append("\n".join(cur).rstrip("\n"))

    for line in m.group(1).split("\n"):
        if line.startswith("    ") and not line.startswith("     "):
            flush()
            cur = [line]
        else:
            cur.append(line)
    flush()
    return [s for s in stmts if s.strip()]


def mutable_map_names(body):
    """Binding names the splice strategy may touch: declared map values and
    their aliases-of-slots (`m := map[...]`, `v := m`)."""
    names = set(re.findall(r"(\w+)\s*:=\s*map\[", body))
    return sorted(names)


def mutate(rng, src, kind=None):
    """Return (mutant_source, kind) or (None, reason). Deterministic in rng."""
    kind = kind or rng.choice(["perm", "dup", "splice"])
    body = split_main_body(src)
    if not body:
        return None, "no-main"
    if kind in ("perm", "dup"):
        body2 = body[:]
        if kind == "perm":
            if len(body2) < 2:
                return None, "too-small"
            k = rng.randrange(1, len(body2) + 1)
            body2 = body2[-k:] + body2[:-k]
        else:
            # never duplicate a declaration: Go refuses a second `:=`
            safe = [s for s in body if ":=" not in s]
            if not safe:
                return None, "no-dupable"
            body2.insert(rng.randrange(len(body2) + 1), rng.choice(safe))
        new_body = "\n".join(body2)
        return MAIN_RE.sub(
            lambda mm: "fn main() {\n" + new_body + "\n}\n", src), kind
    # splice: reuse an existing declared map binding, add wiring if needed
    maps = mutable_map_names("\n".join(body))
    if not maps:
        return None, "no-gc-binding"
    tpl = rng.choice(SPLICE_TPL).format(u=rng.choice(maps))
    idx = rng.randrange(len(body) + 1)
    body3, wiring = list(body), []
    if "k_hz" in tpl:
        # task worker over {u}: kill-channel release happens after every
        # main-body statement, so the worker keeps writing through main's map
        wiring += ["k_hz := make(chan bool, 1)", tpl]
        body3.append("k_hz <- true")
    elif "chz" in tpl:
        # move/plain send across ownership: the receiver stores into it
        wiring += ["chz := make(chan map[string]int, 1)",
                   "done_hz := make(chan bool, 1)",
                   "task hz_recvr(chz, done_hz)"]
        body3.append("<-done_hz")
    body3[idx:idx] = wiring
    new_body = "\n".join(body3)
    return HELPERS + MAIN_RE.sub(
        lambda mm: "fn main() {\n" + new_body + "\n}\n", src), "splice"


def checker_accepts(hyb, path, workdir, timeout):
    """Ray: accepted = clean exit 0 and no HYB-E diagnostic id in the output."""
    try:
        r = subprocess.run([hyb, "check", path], cwd=workdir,
                           capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None, "HANG"
    except OSError as err:
        return None, f"EXEC_ERROR: {err}"
    blob = r.stdout + r.stderr
    if any(b in blob for b in BAD_TEXT):
        return None, f"ICE: {blob[:120]}"
    if r.returncode not in (0, 1):
        return None, f"UNEXPECTED exit={r.returncode}"
    return (r.returncode == 0 and "HYB-E" not in blob), None


def race_oracle(hyb, go_bin, src, workdir, case_timeout):
    """Full leg for one mutant: check → emit-go → go build -race → run.
    Returns (verdict, detail); verdict is RACE / MAPRACE only when the
    trusted checker accepted the mutant."""
    path = os.path.join(workdir, "case.hy")
    with open(path, "w", encoding="utf-8") as f:
        f.write(src)
    accepted, bad = checker_accepts(hyb, path, workdir, case_timeout)
    if bad:
        return f"checker-{bad}", ""
    if not accepted:
        return "reject", ""

    built = subprocess.run([hyb, "build", path, "--emit-go", "-o", "case"],
                           cwd=workdir, capture_output=True, text=True,
                           timeout=case_timeout * 3)
    if built.returncode != 0:
        return "emit-fail", (built.stderr or built.stdout)[-300:]
    golines = sorted(
        f for f in os.listdir(workdir) if f.endswith(".go"))
    if not golines:
        return "emit-fail", "no .go produced"
    env = dict(os.environ)
    env["GOFLAGS"] = env.get("GOFLAGS", "-p=2")
    gb = subprocess.run([go_bin, "build", "-race", "-o", "case.bin"] + golines,
                        cwd=workdir, capture_output=True, text=True,
                        timeout=case_timeout * 3, env=env)
    if gb.returncode != 0:
        # a bridge import is not flat-buildable: reachable but out of scope
        if "not in std" in gb.stderr or "no required module" in gb.stderr:
            return "skip-bridge", gb.stderr[-300:]
        return "gobuild-fail", gb.stderr[-500:]
    run = subprocess.run(["./case.bin"], cwd=workdir, capture_output=True,
                         text=True, timeout=case_timeout * 3, env=env)
    err = (run.stderr or "") + (run.stdout or "")
    if "WARNING: DATA RACE" in err:
        return "RACE", err[-2500:]
    if "concurrent map" in err:
        return "MAPRACE", err[-1500:]
    if run.returncode not in (0, 2):
        return "run-nonzero", err[-400:]
    return "accept-clean", ""


def save_failure(run_dir, seed, index, kind, verdict, detail, src):
    """Data-only reproducer: source + sha256 + oracle tail, exclusive create."""
    encoded = src.encode("utf-8")
    filename = f"case-{index:06d}.hy"
    with open(os.path.join(run_dir, filename), "xb") as f:
        f.write(encoded)
    meta = {
        "version": 1, "seed": seed, "case_index": index, "kind": kind,
        "verdict": verdict, "oracle_tail": detail[-2000:],
        "source_file": filename,
        "source_sha256": hashlib.sha256(encoded).hexdigest(),
    }
    with open(os.path.join(run_dir, f"case-{index:06d}.json"), "x",
              encoding="utf-8") as f:
        json.dump(meta, f, indent=2, sort_keys=True)
        f.write("\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--hyb", required=True)
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--go", default="go",
                    help="race-detector toolchain (default: PATH go)")
    ap.add_argument("--cases", type=int, default=400)
    ap.add_argument("--seed", type=int, default=20261103)
    ap.add_argument("--timeout", type=float, default=20.0)
    ap.add_argument("--workers", type=int, default=2)
    ap.add_argument("--failures-dir", default=None, metavar="PATH",
                    help="save race reproducers (source + metadata) under PATH")
    args = ap.parse_args()

    args.hyb = os.path.abspath(args.hyb)
    go_bin = shutil.which(args.go) or (
        args.go if os.path.isfile(args.go) else "")
    if not os.path.isfile(args.hyb):
        raise SystemExit(f"hyfuzz_race: --hyb binary not found: {args.hyb}")
    if not os.path.isdir(args.corpus):
        raise SystemExit(f"hyfuzz_race: --corpus directory not found: "
                         f"{args.corpus}")
    if not go_bin:
        raise SystemExit(f"hyfuzz_race: --go binary not found: {args.go}")
    if args.cases <= 0:
        raise SystemExit("hyfuzz_race: --cases must be positive")
    if not (math.isfinite(args.timeout) and args.timeout > 0):
        raise SystemExit("hyfuzz_race: --timeout must be positive")
    accepts = load_accepts(args.corpus)
    if not accepts:
        raise SystemExit("hyfuzz_race: no ACCEPT corpus programs found")

    stats = {}
    races = []

    def case(i):
        rng = random.Random((args.seed << 20) + i)
        src, kind = mutate(rng, rng.choice(accepts))
        if src is None:
            return i, kind, None
        with tempfile.TemporaryDirectory(prefix="hyfuzz-race-") as workdir:
            verdict, detail = race_oracle(args.hyb, go_bin, src, workdir,
                                          args.timeout)
        return i, kind, (verdict, detail, src)

    with ThreadPoolExecutor(max_workers=max(1, args.workers)) as ex:
        for i, kind, res in ex.map(case, range(args.cases)):
            if res is None:
                v = f"skip-{kind}"
            else:
                v = res[0]
            stats[v] = stats.get(v, 0) + 1
            if v in ("RACE", "MAPRACE"):
                races.append((i, kind, res))
                if args.failures_dir:
                    os.makedirs(args.failures_dir, exist_ok=True)
                    run_dir = tempfile.mkdtemp(prefix="hyfuzz-race-",
                                               dir=args.failures_dir)
                    save_failure(run_dir, args.seed, i, kind,
                                 res[0], res[1], res[2])
                    print(f"hyfuzz_race: reproducer saved: {run_dir}")
                print(f"hyfuzz_race: case {i} (kind {kind}): "
                      f"{v}\n    {res[1][-400:]!r}", flush=True)

    print(f"hyfuzz_race: {args.cases} cases, seed={args.seed}, "
          + ", ".join(f"{k}={v}" for k, v in sorted(stats.items())))
    if races:
        raise SystemExit(1)
    print("hyfuzz_race: no race surfaced in accepted mutants")


if __name__ == "__main__":
    main()
