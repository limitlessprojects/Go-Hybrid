#!/usr/bin/env python3
"""corpus_harness.py — the Tier-0 checker harness (PLAN.md Phase-1 item).

Consumes tests/checker-corpus/*.hy: reads each program's pre-registered
`// EXPECTED-VERDICT:` line (the corpus IS the spec — human ruling Q1,
addition 2), runs `hyb check --json` over the file, and diffs the checker's
verdict against the registered one.  Also recomputes the managed tripwire
baseline (Q1 addition 5, definitions fixed in VERDICTS.md BEFORE the checker
existed — verdict-level and as-written) and aggregates --report-managed data.

Verdict agreement rules (VERDICTS.md "checker consumption"):
  EXPECTED  ACCEPT           -> got.verdict == accept AND zero errors
  EXPECTED  ACCEPT-MANAGED   -> got.verdict == accept AND >= 1 managed lint
                                (HYB-E8001) AND zero errors
  EXPECTED  REJECT <rule-id> -> got.verdict == reject AND the first ERROR
                                diagnostic carries tier0_rule == <rule-id>
  Optional EXPECTED-DIAGNOSTIC: HYB-Exxxx on a REJECT fixture additionally
                                pins that first ERROR's exact code.

Any disagreement is a bug in the checker OR in the corpus, resolved by a
logged amendment to VERDICTS.md — never silently (pre-registration stmt).

Exit codes: 0 = full agreement; 1 = disagreement; 2 = harness misuse.
"""

import argparse
import json
import os
import re
import subprocess
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_CORPUS = os.path.join(REPO_ROOT, "tests", "checker-corpus")
DEFAULT_BIN = os.path.join(os.environ.get("CARGO_TARGET_DIR", "/home/z/go-hybrid-target"), "debug", "hyb")

VERDICT_RE = re.compile(
    r"^//[ \t]*EXPECTED-VERDICT:[ \t]*(ACCEPT-MANAGED|ACCEPT|REJECT)"
    r"(?![-\w])(?:[ \t]+([^\s()]+))?(?:[ \t]+\([^)]*\))?[ \t]*$", re.M
)
VERDICT_HEADER_RE = re.compile(
    r"^[ \t]*//[ \t]*EXPECTED-VERDICT\b[^\n]*$", re.M
)
DIAGNOSTIC_HEADER_RE = re.compile(
    r"^[ \t]*//[ \t]*EXPECTED-DIAGNOSTIC\b[^\n]*$", re.M
)
DIAGNOSTIC_RE = re.compile(r"//[ \t]*EXPECTED-DIAGNOSTIC:[ \t]*(HYB-E[0-9]{4})[ \t]*")

# As-written managed (the friction number) — the pre-registered static list
# from VERDICTS.md "Tripwire baseline" (04 counts only while lock-mediated
# sharing stays outside Tier 0, OQ2):
AS_WRITTEN_MANAGED = {"01", "03", "04"}
TRIPWIRE_PCT = 25.0  # ~25%, recorded tunable (SPEC §9.7)


def read_expected(path):
    """Return (verdict, rule_id) or None if no EXPECTED-VERDICT line."""
    try:
        with open(path, encoding="utf-8") as f:
            src = f.read()
    except (OSError, UnicodeDecodeError) as exc:
        raise ValueError(f"corpus error: {path}: cannot read corpus file: {exc}") from exc
    headers = VERDICT_HEADER_RE.findall(src)
    if not headers:
        return None
    if len(headers) != 1:
        raise ValueError(f"corpus error: {path} has multiple EXPECTED-VERDICT headers")
    m = VERDICT_RE.search(headers[0].strip())
    if m is None:
        raise ValueError(f"corpus error: {path} has malformed EXPECTED-VERDICT header")
    verdict, rule = m.group(1), m.group(2)
    if verdict == "REJECT" and not rule:
        raise ValueError(f"corpus error: {path} has REJECT without a rule id")
    if verdict != "REJECT" and rule:
        raise ValueError(f"corpus error: {path} has unexpected rule after {verdict}")
    return verdict, (rule or None)


def read_expected_diagnostic(path, expected):
    """Read an optional exact first-error code, rejecting malformed metadata.

    Only REJECT fixtures may declare this header. Existing fixtures without
    it keep their verdict/rule-only contract; read_expected's API is unchanged.
    """
    try:
        with open(path, encoding="utf-8") as f:
            headers = DIAGNOSTIC_HEADER_RE.findall(f.read())
    except (OSError, UnicodeDecodeError) as exc:
        raise ValueError(f"corpus error: {path}: cannot read corpus file: {exc}") from exc
    if not headers:
        return None
    if len(headers) != 1:
        raise ValueError(f"corpus error: {path} has multiple EXPECTED-DIAGNOSTIC headers")
    match = DIAGNOSTIC_RE.fullmatch(headers[0].strip())
    if match is None:
        raise ValueError(f"corpus error: {path} has malformed EXPECTED-DIAGNOSTIC header")
    if expected is None or expected[0] != "REJECT":
        raise ValueError(f"corpus error: {path} declares EXPECTED-DIAGNOSTIC without REJECT")
    return match.group(1)


def agree_diagnostic(expected_code, diags):
    """Compare an optional exact first-error code, independently of its rule."""
    if expected_code is None:
        return True, ""
    errors = [d for d in diags if d.get("severity") == "error"]
    actual = errors[0].get("code") if errors else None
    return actual == expected_code, f"expected first diagnostic {expected_code}, got {actual}"


def run_checker(bin_path, hy_path, report_managed=False):
    """Run `hyb check --json` over one file; return (summary, diagnostics)."""
    cmd = [bin_path, "check", hy_path, "--json"]
    if report_managed:
        cmd.append("--report-managed")
    # The checker emits UTF-8 JSON, not text in the caller's locale. Decode
    # after communicate(): on Windows, a TextIOWrapper decoding error in a
    # reader thread can otherwise leave stdout/stderr as None.
    try:
        proc = subprocess.run(cmd, capture_output=True, timeout=60)
    except subprocess.TimeoutExpired as e:
        # A checker that hangs must not abort the remaining corpus rows:
        # surface the timeout as a per-case harness-error row and keep
        # running the rest of the corpus (pr-198, shoujianing).
        raise RuntimeError(
            f"checker timed out for {hy_path} after {e.timeout} seconds"
        ) from e
    except OSError as e:
        # The validated file may still fail to execute (not executable,
        # wrong architecture, vanished between validation and exec).
        # Surface it as a harness-error row, never a traceback.
        raise RuntimeError(f"checker could not be executed for {hy_path}: {e}")
    try:
        stdout = proc.stdout.decode("utf-8")
        stderr = proc.stderr.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise RuntimeError(f"checker output is not valid UTF-8 for {hy_path}: {exc}") from exc
    # Exit 0 means accepted and exit 1 means rejected. Any other status
    # indicates a checker failure, even if it already emitted valid JSON.
    if proc.returncode not in (0, 1):
        raise RuntimeError(
            f"checker exited unexpectedly for {hy_path} "
            f"(exit {proc.returncode}): {stderr[:400]}"
        )
    out = stdout.strip()
    if not out:
        raise RuntimeError(
            f"checker produced no JSON for {hy_path} (exit {proc.returncode}): {stderr[:400]}"
        )
    try:
        payload = json.loads(out)
    except json.JSONDecodeError as e:
        raise RuntimeError(f"checker JSON invalid for {hy_path}: {e}: {out[:400]}")
    if not isinstance(payload, dict):
        raise RuntimeError(f"checker JSON envelope invalid for {hy_path}: expected an object")
    summary = payload.get("summary", {})
    diags = payload.get("diagnostics", [])
    if not isinstance(summary, dict) or not isinstance(diags, list) or any(
        not isinstance(d, dict) for d in diags
    ):
        raise RuntimeError(f"checker JSON envelope invalid for {hy_path}: summary/diagnostics")
    # Older fixtures may omit counts. When supplied, they must be integer
    # counts of the actual diagnostics, not contradictory evidence of a pass.
    for field, severity in (("errors", "error"), ("warnings", "warning")):
        if field in summary:
            count = summary[field]
            actual = sum(d.get("severity") == severity for d in diags)
            if type(count) is not int or count < 0 or count != actual:
                raise RuntimeError(
                    f"checker summary inconsistent for {hy_path}: "
                    f"{field}={count!r}, diagnostics={actual}"
                )
    verdict = summary.get("verdict")
    expected_exit = {"accept": 0, "reject": 1}.get(verdict)
    if expected_exit is not None and proc.returncode != expected_exit:
        raise RuntimeError(
            f"checker exit/verdict mismatch for {hy_path}: "
            f"exit {proc.returncode}, verdict {verdict}"
        )
    if report_managed:
        managed = summary.get("managed")
        # Parse failures have no managed report. A report that is present
        # must carry the usize counters emitted by hy_diag::ManagedReport;
        # silently defaulting malformed counters to zero hides audit drift.
        if managed is not None:
            if not isinstance(managed, dict):
                raise RuntimeError(f"checker managed report invalid for {hy_path}: expected an object")
            for field in ("managed_blocks", "sites_via_managed", "total_sites"):
                count = managed.get(field)
                if type(count) is not int or count < 0:
                    raise RuntimeError(
                        f"checker managed report invalid for {hy_path}: "
                        f"{field} must be a nonnegative integer"
                    )
            if managed["sites_via_managed"] > managed["total_sites"]:
                raise RuntimeError(
                    f"checker managed report inconsistent for {hy_path}: "
                    "sites_via_managed exceeds total_sites"
                )
    return summary, diags


def classify_got(summary, diags):
    """Turn checker output into a comparable (verdict, rule, managed_lint)."""
    errors = [d for d in diags if d.get("severity") == "error"]
    warnings = [d for d in diags if d.get("severity") == "warning"]
    managed_lint = [d for d in warnings if d.get("code") == "HYB-E8001"]
    verdict = summary.get("verdict", "?")
    # Parser/type errors need not carry a Tier-0 rule. None must mean no
    # errors, otherwise an inconsistent accept summary can pass the corpus.
    if errors:
        raw_rule = errors[0].get("tier0_rule")
        first_rule = (
            raw_rule.strip()
            if isinstance(raw_rule, str) and raw_rule.strip()
            else "<unclassified-error>"
        )
    else:
        first_rule = None
    return verdict, first_rule, managed_lint


def agree(expected, got):
    """expected: (verdict, rule|None); got: (verdict, rule|None, managed_lint)."""
    ev, er = expected
    gv, gr, managed_lint = got
    if ev == "ACCEPT":
        return gv == "accept" and gr is None, "expected clean accept"
    if ev == "ACCEPT-MANAGED":
        return gv == "accept" and gr is None and len(managed_lint) >= 1, (
            "expected accept + managed lint"
        )
    if ev == "REJECT":
        if (
            gv != "reject"
            or not isinstance(gr, str)
            or not gr.strip()
            or gr == "<unclassified-error>"
        ):
            return False, f"expected reject {er}, got {gv} with rule {gr}"
        return gr == er, f"expected reject {er}, got reject {gr}"
    return False, "unknown expected verdict"


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--bin", default=DEFAULT_BIN, help="path to the hyb binary")
    ap.add_argument("--corpus", default=DEFAULT_CORPUS, help="corpus directory")
    ap.add_argument("--report-managed", action="store_true", help="aggregate --report-managed data")
    ap.add_argument("--verbose", action="store_true", help="print every diagnostic on mismatch")
    args = ap.parse_args()

    if not os.path.isfile(args.bin):
        print(f"error: checker binary not found: {args.bin}", file=sys.stderr)
        print("build it first: cd compiler && cargo build -p hyb", file=sys.stderr)
        return 2

    # The validated local file must be the binary that actually runs. An
    # unresolved name (a bare `hyb`, a relative `./hyb`) would let
    # subprocess's PATH lookup silently substitute a different installed
    # checker — a false agreement or a false disagreement — and raise
    # FileNotFoundError when PATH carries no such binary at all.
    args.bin = os.path.abspath(args.bin)

    try:
        entries = os.listdir(args.corpus)
    except OSError as exc:
        print(f"error: corpus directory not readable: {args.corpus}: {exc}", file=sys.stderr)
        return 2
    files = sorted(
        f for f in entries if f.endswith(".hy")
    )
    if not files:
        print(f"error: no .hy files in {args.corpus}", file=sys.stderr)
        return 2

    total = 0
    agreed = 0
    rows = []
    managed_blocks = 0
    managed_sites = 0
    total_sites = 0
    accept_managed_count = 0
    expected_diagnostics = {}
    got_diagnostics = {}

    for fname in files:
        path = os.path.join(args.corpus, fname)
        try:
            expected = read_expected(path)
            expected_code = read_expected_diagnostic(path, expected)
        except ValueError as e:
            print(str(e), file=sys.stderr)
            return 2
        expected_diagnostics[fname] = expected_code
        if expected is None:
            print(f"SKIP (no EXPECTED-VERDICT): {fname}")
            continue
        total += 1
        num = fname.split("-", 1)[0]
        try:
            summary, diags = run_checker(args.bin, path, args.report_managed)
        except RuntimeError as e:
            rows.append((fname, expected, ("harness-error", None, []), False, str(e)))
            continue
        got = classify_got(summary, diags)
        ok, why = agree(expected, got)
        code_ok, code_why = agree_diagnostic(expected_code, diags)
        if not code_ok:
            why += "; " + code_why
        ok = ok and code_ok
        errors = [d for d in diags if d.get("severity") == "error"]
        got_diagnostics[fname] = errors[0].get("code") if errors else None
        if ok:
            agreed += 1
        if expected[0] == "ACCEPT-MANAGED":
            accept_managed_count += 1
        if args.report_managed:
            m = summary.get("managed") or {}
            managed_blocks += m.get("managed_blocks", 0)
            managed_sites += m.get("sites_via_managed", 0)
            total_sites += m.get("total_sites", 0)
        detail = why if not ok else ""
        if not ok and args.verbose:
            detail += "\n    " + "\n    ".join(
                f"{d.get('severity')}[{d.get('code')}] {d.get('message')} (rule={d.get('tier0_rule')})"
                for d in diags[:6]
            )
        rows.append((fname, expected, got[:2], ok, detail))

    if total == 0:
        print("error: no registered EXPECTED-VERDICT cases in corpus", file=sys.stderr)
        return 2

    # ---- report ----
    print(f"{'program':<46} {'expected':<28} {'got':<28} {'ok':<4}")
    print("-" * 110)
    for fname, expected, got, ok, detail in rows:
        exp_s = expected[0] + (f" {expected[1]}" if expected[1] else "")
        got_s = got[0] + (f" {got[1]}" if got[1] else "")
        if expected_diagnostics.get(fname):
            exp_s += f" [{expected_diagnostics[fname]}]"
            got_s += f" [{got_diagnostics.get(fname)}]"
        mark = "OK" if ok else "FAIL"
        print(f"{fname:<46} {exp_s:<28} {got_s:<28} {mark:<4}")
        if detail:
            print(f"    {detail}")
    print("-" * 110)
    print(f"verdict agreement: {agreed}/{total}")

    # tripwire recompute (definitions fixed in VERDICTS.md)
    verdict_level = 100.0 * accept_managed_count / total if total else 0.0
    as_written = 100.0 * len(AS_WRITTEN_MANAGED) / total if total else 0.0
    print(f"tripwire (verdict-level managed): {accept_managed_count}/{total} = {verdict_level:.1f}%")
    print(f"tripwire (as-written managed):    {len(AS_WRITTEN_MANAGED)}/{total} = {as_written:.1f}%")
    print(f"tripwire threshold: ~{TRIPWIRE_PCT}% (tunable, SPEC §9.7)")
    if verdict_level > TRIPWIRE_PCT or as_written > TRIPWIRE_PCT:
        print("TRIPWIRE FIRED: reopen G0 with the §9.4 alternatives table")
    else:
        print("tripwire: not fired")

    if args.report_managed:
        pct = 100.0 * managed_sites / total_sites if total_sites else 0.0
        print(
            f"managed aggregate: {managed_blocks} block(s), "
            f"{managed_sites}/{total_sites} sites via managed = {pct:.1f}%"
        )

    return 0 if agreed == total else 1


if __name__ == "__main__":
    sys.exit(main())
