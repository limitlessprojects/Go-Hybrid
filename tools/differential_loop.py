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
      with exit 0, and match EXPECTED-OUTPUT when registered. A checker
      accept that cannot execute is a false
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
OUTPUT_HEADER_RE = re.compile(r"^//[ \t]*EXPECTED-OUTPUT\b[^\r\n]*", re.M)
OUTPUT_RE = re.compile(r"//[ \t]*EXPECTED-OUTPUT:[ \t]*(.*)")


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


def read_expected_output(path, expected):
    """Read an optional single-line stdout assertion without changing verdicts.

    Annotation padding is trimmed. Output may have one terminal newline
    (LF or CRLF); other whitespace and extra lines remain significant.
    """
    with open(path, encoding="utf-8") as f:
        headers = OUTPUT_HEADER_RE.findall(f.read())
    if not headers:
        return None
    if len(headers) != 1:
        raise SystemExit(f"corpus error: {path} has multiple EXPECTED-OUTPUT markers")
    match = OUTPUT_RE.fullmatch(headers[0])
    if match is None or not match.group(1).strip():
        raise SystemExit(f"corpus error: {path} has malformed EXPECTED-OUTPUT marker")
    if expected is None or expected[0] not in ("ACCEPT", "ACCEPT-MANAGED"):
        raise SystemExit(f"corpus error: {path} declares EXPECTED-OUTPUT without ACCEPT")
    return match.group(1).strip()


def checker_rejects(bin_path, hy_path, expected_rule):
    """Require the first error to match the pre-registered rejection rule.

    False also covers harness faults — an unexecutable checker, a decode
    failure, or undecodable output — not only checker disagreement: the
    differential loop must never crash on them, and never count them as
    agreement.
    """
    # The checker emits UTF-8 JSON, not text in the caller's locale. With
    # text=True on a non-UTF-8 host (Windows cp936) the locale decode
    # raises out of run() itself, or leaves the stream None in a reader
    # thread; capture bytes and decode here so both become a reportable
    # (False, detail) instead of a traceback — the same failure class as
    # corpus_harness.run_checker (#492).
    try:
        proc = subprocess.run(
            [bin_path, "check", hy_path, "--json"],
            capture_output=True, timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return False, f"checker could not be run: {exc}"
    if proc.returncode not in (0, 1):
        detail = f"unexpected checker exit {proc.returncode}"
        if proc.stderr:
            detail += ": " + proc.stderr.decode("utf-8", "replace").strip()[-200:]
        return False, detail
    try:
        out = proc.stdout.decode("utf-8").lstrip("\ufeff").strip()
    except UnicodeDecodeError as exc:
        detail = f"checker output is not valid UTF-8: {exc}"
        if proc.stderr:
            detail += ": " + proc.stderr.decode("utf-8", "replace").strip()[-200:]
        return False, detail
    try:
        payload = json.loads(out)
    except json.JSONDecodeError:
        return False, "no/invalid checker JSON"
    # Envelope validation (PR #260): a well-formed JSON scalar/list, a
    # non-dict summary, non-list diagnostics, or non-dict diagnostic items
    # used to escape as AttributeError tracebacks; a summary "errors" count
    # contradicting the diagnostics was silently accepted (#601 extended the
    # count check to "warnings" too).
    if not isinstance(payload, dict):
        return False, "invalid checker JSON envelope: expected an object"
    summary = payload.get("summary", {})
    diags = payload.get("diagnostics", [])
    if not isinstance(summary, dict) or not isinstance(diags, list) or any(
        not isinstance(d, dict) for d in diags
    ):
        return False, "invalid checker JSON envelope: summary/diagnostics"
    # Match the corpus harness's protocol checks (PR #260 + its #601 residual):
    # a contradictory error OR warning count cannot establish rejection
    # agreement, even with a matching rule. Older checker fixtures may omit
    # counts entirely.
    for field, severity in (("errors", "error"), ("warnings", "warning")):
        if field in summary:
            count = summary[field]
            actual = sum(d.get("severity") == severity for d in diags)
            if type(count) is not int or count < 0 or count != actual:
                return False, f"inconsistent summary {field} count: {count!r} vs {actual}"
    if proc.returncode != 1:
        return False, (
            f"checker exit/verdict mismatch: exit {proc.returncode}, "
            f"verdict={summary.get('verdict')}"
        )
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

    Go >= 1.26 (swiss-map runtime) reports the same torn concurrent map
    write under several first-line wordings: the classic
    "fatal error: concurrent map writes", a self-attributing corruption
    fatal ("small map with no empty slot (concurrent map writes?)"), and
    an unattributed "fatal error: unexpected fault address ..." raised
    from the slot the torn write corrupted. All three carry the Go fatal
    exit code and are the same pre-registered hazard class, so the
    matcher (a) recognizes the registered message as a substring of a
    `fatal error:` first line, and (b) treats the fault-address fatal as
    the concurrent-map class only when a concurrent-map hazard was
    registered. A `panic:` line still matches by exact wording only — a
    panic that merely mentions the hazard phrase is not evidence.
    """
    first_line = next((line.strip().lower() for line in stderr.splitlines()
                       if line.strip()), "")
    for hazard in hazards:
        message = hazard.split("(", 1)[0].strip().lower()
        if not message:
            continue
        if first_line in (f"fatal error: {message}", f"panic: {message}"):
            return True
        if first_line.startswith("fatal error:") and message in first_line:
            return True
        if message.startswith("concurrent map") and first_line.startswith(
                "fatal error: unexpected fault address"):
            return True
    return False


def build_and_run(bin_path, hy_path, workdir, hazards=(), expected_output=None):
    """Full pipeline: hyb build + run the binary. Returns (ok, detail)."""
    # TimeoutExpired is a per-case harness fault like any other build/run
    # failure (pr-468): report the case, keep the corpus running, and let
    # the aggregate report carry the failure — never abort the loop.
    try:
        build = subprocess.run(
            [bin_path, "build", hy_path],
            cwd=workdir, capture_output=True, text=True, timeout=300,
        )
    except subprocess.TimeoutExpired as exc:
        return False, f"build timed out after {exc.timeout}s"
    except OSError as exc:
        return False, f"builder could not be run: {exc}"
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
    except OSError as exc:
        return False, f"program could not be run: {exc}"
    detail = "exit 0"
    if run.returncode != 0:
        # A Go fatal normally exits 2; GOTRACEBACK=crash aborts on Unix.
        # Matching text must not hide a different exit or a killed process.
        go_fatal_exit = run.returncode == 2 or (
            os.name == "posix" and run.returncode == -signal.SIGABRT
        )
        if go_fatal_exit and documented_runtime_hazard(run.stderr, hazards):
            detail = "documented RUNTIME-HAZARD fatal (pre-registered)"
        else:
            return False, f"run exit {run.returncode}: {(run.stderr or run.stdout)[-200:]}"
    if expected_output is not None:
        stdout = run.stdout.replace("\r\n", "\n")
        if stdout not in (expected_output, expected_output + "\n"):
            return False, (f"stdout mismatch: expected {expected_output[:200]!r}, "
                           f"got {run.stdout[:200]!r}")
    return True, detail


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
        expected_output = read_expected_output(path, expected)
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
                ok, detail = build_and_run(
                    bin_path, path, workdir, hazards, expected_output=expected_output
                )
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
    if accept_total + reject_total == 0:
        print("differential: no registered EXPECTED-VERDICT cases in corpus",
              file=sys.stderr)
        return 2
    if accept_pass == accept_total and reject_pass == reject_total:
        print("differential: FULL AGREEMENT")
        return 0
    print(f"differential: {len(failures)} disagreement(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main())
