#!/usr/bin/env python3
"""differential_loop.py — the differential corpus loop (SPEC §"Differential
tests on the corpus — track pass rate in PLAN.md"; PLAN.md Phase-1/2 item).

The Tier-0 corpus is pre-registered with EXPECTED-VERDICT lines (the corpus
IS the spec — human ruling Q1). corpus_harness.py pins the CHECKER side of
that contract; THIS harness pins the PIPELINE side:

  EXPECTED ACCEPT / ACCEPT-MANAGED
      the program must survive the FULL pipeline — `hyb build` (parse →
      check → emit → go build, assembling the bridge workspace when the
      program crosses the rust boundary) — and the built binary must run
      with exit 0. A checker accept that cannot execute is a false
      positive: an emitter bug or a surface gap. (Batch I landed exactly
      this way: the loop exposed seven distinct emission gaps, all fixed.)

  EXPECTED REJECT <rule-id>
      the checker must reject — re-verified here so one command covers
      both halves of the differential. (The pipeline never runs a
      rejected program: that is the point of the rejection.)

Pass rate is printed as accept-side runs / accept-side total; PLAN.md
tracks it. Exit codes: 0 = full agreement; 1 = disagreement; 2 = misuse.
"""

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_CORPUS = os.path.join(REPO_ROOT, "tests", "checker-corpus")
DEFAULT_BIN = os.path.join(
    os.environ.get("CARGO_TARGET_DIR", "/home/z/go-hybrid-target"), "debug", "hyb"
)

# Detect declarations before validating their value: a typo must not turn
# a registered test into an unregistered helper. Keep parsing on one line.
VERDICT_RE = re.compile(
    r"^//[ \t]*EXPECTED-VERDICT:[ \t]*([^\r\n]*)", re.M
)
HAZARD_RE = re.compile(
    r"^//\s*RUNTIME-HAZARD:\s*(.+)$", re.M
)


def read_expected(path):
    """(verdict, rule, hazards) — hazards are the program's pre-registered
    RUNTIME-HAZARD markers (corpus honesty notes made machine-readable): an
    accept-side run may legally die of the documented Go runtime fatal
    (e.g. corpus 02's deliberate concurrent map writes)."""
    with open(path, encoding="utf-8") as f:
        src = f.read()
    markers = VERDICT_RE.findall(src)
    if not markers:
        return None
    if len(markers) != 1:
        raise SystemExit(f"corpus error: {path} has multiple EXPECTED-VERDICT markers")
    fields = markers[0].split()
    if not fields or fields[0] not in ("ACCEPT-MANAGED", "ACCEPT", "REJECT"):
        raise SystemExit(f"corpus error: {path} has invalid EXPECTED-VERDICT: {markers[0]!r}")
    verdict = fields[0]
    rule = fields[1] if verdict == "REJECT" and len(fields) > 1 else None
    if verdict == "REJECT" and not rule:
        raise SystemExit(f"corpus error: {path} has REJECT without a rule id")
    hazards = [h.strip().lower() for h in HAZARD_RE.findall(src)]
    return verdict, (rule or None), hazards


def checker_rejects(bin_path, hy_path, expected_rule):
    """Require the first error to match the pre-registered rejection rule."""
    try:
        proc = subprocess.run(
            [bin_path, "check", hy_path, "--json"],
            capture_output=True, text=True, timeout=60,
        )
    except subprocess.TimeoutExpired as exc:
        return False, f"checker timed out after {exc.timeout}s"
    if proc.returncode not in (0, 1):
        return False, f"unexpected checker exit {proc.returncode}"
    try:
        payload = json.loads(proc.stdout.strip())
    except json.JSONDecodeError:
        return False, "no/invalid checker JSON"
    summary = payload.get("summary", {})
    if proc.returncode != 1:
        return False, (
            f"checker exit/verdict mismatch: exit {proc.returncode}, "
            f"verdict={summary.get('verdict')}"
        )
    diags = payload.get("diagnostics", [])
    errors = [d for d in diags if d.get("severity") == "error"]
    first_rule = errors[0].get("tier0_rule") if errors else None
    return (
        summary.get("verdict") == "reject"
        and first_rule is not None and first_rule == expected_rule,
        f"verdict={summary.get('verdict')} errors={len(errors)} "
        f"first_rule={first_rule} expected_rule={expected_rule}",
    )


def documented_runtime_hazard(stderr, hazards):
    """Match the runtime's first diagnostic against a registered fatal.

    Inspect the complete stderr before display truncation: Go stack traces
    may exceed the report's tail limit. Generic panic/fatal output is not
    evidence that the specific pre-registered hazard occurred.
    """
    first_line = next((line.strip().lower() for line in stderr.splitlines()
                       if line.strip()), "")
    for hazard in hazards:
        message = hazard.split("(", 1)[0].strip().lower()
        if message and first_line in (f"fatal error: {message}", f"panic: {message}"):
            return True
    return False


def build_and_run(bin_path, hy_path, workdir, hazards=()):
    """Full pipeline: hyb build + run the binary. Returns (ok, detail)."""
    try:
        build = subprocess.run(
            [bin_path, "build", hy_path],
            cwd=workdir, capture_output=True, text=True, timeout=300,
        )
    except subprocess.TimeoutExpired as exc:
        return False, f"build timed out after {exc.timeout}s"
    if build.returncode != 0:
        return False, "build failed: " + (build.stderr or build.stdout).strip()[-300:]
    binary = os.path.join(workdir, "a.out")
    if not os.path.exists(binary):
        return False, "no a.out produced"
    try:
        run = subprocess.run(
            [binary], cwd=workdir, capture_output=True, text=True, timeout=60
        )
    except subprocess.TimeoutExpired as exc:
        return False, f"run timed out after {exc.timeout}s"
    if run.returncode != 0:
        # A Go fatal normally exits 2; GOTRACEBACK=crash aborts on Unix.
        # Matching text must not hide a different exit or a killed process.
        go_fatal_exit = run.returncode == 2 or (
            os.name == "posix" and run.returncode == -signal.SIGABRT
        )
        if go_fatal_exit and documented_runtime_hazard(run.stderr, hazards):
            return True, "documented RUNTIME-HAZARD fatal (pre-registered)"
        return False, f"run exit {run.returncode}: {(run.stderr or run.stdout)[-200:]}"
    return True, "exit 0"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default=DEFAULT_CORPUS)
    ap.add_argument("--bin", default=DEFAULT_BIN)
    args = ap.parse_args()
    # The build leg runs with cwd = a temp workspace — a relative --bin
    # or --corpus would resolve THERE (the CI failure this guard fixed).
    # Absolutize once, up front, against the caller's cwd.
    bin_path = os.path.abspath(args.bin)
    corpus_path = os.path.abspath(args.corpus)
    if not os.path.isfile(bin_path):
        raise SystemExit(f"differential: hyb binary not found at {bin_path}")

    files = sorted(
        os.path.join(corpus_path, f)
        for f in os.listdir(corpus_path)
        if f.endswith(".hy")
    )
    if not files:
        raise SystemExit(2)

    accept_pass = accept_total = 0
    reject_pass = reject_total = 0
    failures = []

    for path in files:
        expected = read_expected(path)
        if expected is None:
            continue
        name = os.path.basename(path)
        verdict, rule, hazards = expected
        if verdict == "REJECT":
            reject_total += 1
            ok, detail = checker_rejects(bin_path, path, rule)
            if ok:
                reject_pass += 1
                print(f"PASS  {name} (reject {rule})")
            else:
                failures.append(name)
                print(f"FAIL  {name} (expected reject {rule}; {detail})")
        else:
            accept_total += 1
            workdir = tempfile.mkdtemp(prefix="hydiff-")
            try:
                ok, detail = build_and_run(bin_path, path, workdir, hazards)
            finally:
                shutil.rmtree(workdir, ignore_errors=True)
            if ok:
                accept_pass += 1
                print(f"PASS  {name} ({verdict}: {detail})")
            else:
                failures.append(name)
                print(f"FAIL  {name} ({verdict}: {detail})")

    rate = f"{accept_pass}/{accept_total}"
    print("----")
    print(f"differential accept-side (build+run): {rate}")
    print(f"differential reject-side (checker):   {reject_pass}/{reject_total}")
    if accept_total and accept_pass == accept_total and reject_pass == reject_total:
        print("differential: FULL AGREEMENT")
        return 0
    print(f"differential: {len(failures)} disagreement(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main())
