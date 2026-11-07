#!/usr/bin/env python3
"""guarantees_gate.py — W-2220: execute the claims table, not just render it.

`tools/check_guarantees.py` (issue #710) made `guarantees.json` the source of
the GUARANTEES.md table and `tools/check_claims_links.py` (#2221) verified its
citations. This spike adds the missing half of the W-2220 proposal:

1. **Verifier kinds.** A row's evidence is a typed verifier that the gate
   executes: `corpus_pair` (run the checker on the accept/reject pair),
   `run_gate` (run `tools/managed_gate.py`, including a new-block rejection
   probe), `runtime_pin` (build the emitted Go and pin a runtime panic under
   `-race`), `test_suite` (run a focused suite and check a static invariant),
   and `benchmark` (run a measured bench and require its PASS verdict).
2. **Demotion state machine.** A row is `executable` only while its verifier
   passes. `prose-only` requires an owner, an expiry, and a debt ticket; an
   expired row renders as **demoted to no claim**, never as prose that still
   reads like a guarantee.
3. **Seeded-mutation self-test.** `--self-test` mutates the registry/policy in
   eight ways and fails unless every mutation is caught. A gate that cannot
   fail is not a gate.

M1 scope was rows 1, 8 and 16. M2 applies the production registry to **all 20
rows** through `m2_overrides.json` (spike-local fixtures only; production files
are untouched) and publishes the coverage table: which rows became executable,
which are prose-only with an expiry and why, and whether the pre-registered
wall (>=12 rows not executable) was hit.

Usage:
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --render
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py \
        --hyb compiler/target/debug/hyb --go go

Exit codes: 0 = every executable row verified and both rendered tables match;
1 = a violation or a caught self-test mutation; 2 = CLI misuse.
"""

from __future__ import annotations

import argparse
import copy
import datetime
import json
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SPIKE_DIR = Path(__file__).resolve().parent
REPO_ROOT = SPIKE_DIR.parents[1]

M1_REQUIRED = {1: "corpus_pair", 8: "run_gate", 16: "runtime_pin"}
M2_REQUIRED = set(range(1, 21))
WALL_THRESHOLD = 12
ALLOWED_KINDS = {
    "corpus_pair",
    "run_gate",
    "runtime_pin",
    "test_suite",
    "benchmark",
    "argument",
}
EXECUTABLE_KINDS = {
    "corpus_pair",
    "run_gate",
    "runtime_pin",
    "test_suite",
    "benchmark",
}
EXPECT_TO_OBSERVED = {"reject": "REJECT", "accept": "ACCEPT", "accept-managed": "ACCEPT"}

PROSE_PREFIX = "**No claim (prose-only)**"
DEMOTED_PREFIX = "**Demoted to no claim**"
UNREGISTERED_CLAIM = "unregistered (no policy entry)"
COVERAGE_HEADER = (
    "| Row | Surface | Strength | State | Verifier | Why this verifier / why not executable | "
    "Owner · expiry/debt |"
)


def utc_today() -> datetime.date:
    return datetime.datetime.now(datetime.timezone.utc).date()


def parse_date(value: str) -> datetime.date | None:
    try:
        return datetime.date.fromisoformat(value)
    except (TypeError, ValueError):
        return None


def load_json(path: Path) -> dict:
    with path.open() as fh:
        return json.load(fh)


def claim_rows(claims: dict) -> dict[int, dict]:
    # guarantees.json stores the table under `rows`; accept `claims` too so the
    # spike does not silently read an empty registry on a schema rename.
    raw = claims.get("rows", claims.get("claims", []))
    if not raw:
        raise ValueError("claims registry has no `rows`/`claims` entries")
    return {int(row["id"]): row for row in raw}


def apply_overrides(claims: dict, overlay: dict | None) -> dict:
    """Return a copy of the registry with M2's spike-local fixture additions.

    The production `guarantees.json` is never edited: M2 only adds fixtures
    where the registry has none yet (rows 14/15), so the coverage table can
    distinguish "no executable evidence exists" from "evidence exists but the
    production migration has not happened".
    """
    merged = copy.deepcopy(claims)
    if not overlay:
        return merged
    by_id = {int(row["id"]): row for row in merged.get("rows", [])}
    for rid_s, fixtures in (overlay.get("fixtures") or {}).items():
        row = by_id.get(int(rid_s))
        if row is None:
            raise ValueError(f"overlay names row {rid_s}, which is not in the registry")
        row["fixtures"] = copy.deepcopy(fixtures)
    return merged


def load_claims(root: Path, claims_rel: str, overlay_rel: str | None) -> tuple[dict, dict | None]:
    claims = load_json(root / claims_rel)
    overlay = None
    if overlay_rel:
        overlay_path = root / overlay_rel
        if overlay_path.exists():
            overlay = load_json(overlay_path)
    return apply_overrides(claims, overlay), overlay


def substitute(cmd: list[str], hyb: str, go_bin: str) -> list[str]:
    return [
        str(part).replace("{hyb}", hyb).replace("{go}", go_bin)
        for part in cmd
    ]


def verifier_label(verifier: dict | None) -> str:
    if not verifier:
        return "-"
    kind = verifier.get("kind", "?")
    extra = []
    for key in ("gate", "runtime"):
        if verifier.get(key):
            extra.append(str(verifier[key]))
    command = verifier.get("command")
    if command:
        extra.append(" ".join(str(part) for part in command))
    if verifier.get("expect"):
        extra.append("expect=" + str(verifier["expect"]))
    return kind + ("(" + ", ".join(extra) + ")" if extra else "")


def row_state(rid: int, row: dict, pol: dict | None) -> dict:
    """Return the effective state of one claim row under the demotion policy."""
    state = {
        "id": rid,
        "registered": pol is not None,
        "status": "unregistered",
        "owner": None,
        "expires": None,
        "debt": None,
        "review_by": None,
        "verifier": None,
        "problems": [],
        "effective_claim": UNREGISTERED_CLAIM,
    }
    if pol is None:
        return state

    state["owner"] = pol.get("owner")
    state["expires"] = pol.get("expires")
    state["debt"] = pol.get("debt")
    state["review_by"] = pol.get("review_by")
    verifier = pol.get("verifier")
    state["verifier"] = verifier
    status = pol.get("status")
    strength = row.get("strength")

    if status == "executable":
        kind = (verifier or {}).get("kind")
        if kind not in EXECUTABLE_KINDS:
            state["status"] = "invalid"
            state["problems"].append(
                f"row {rid}: executable status needs a runnable verifier kind "
                f"({sorted(EXECUTABLE_KINDS)}), got {kind!r}"
            )
            return state
        if strength == "guaranteed" and kind == "argument":
            state["status"] = "invalid"
            state["problems"].append(f"row {rid}: a guaranteed claim cannot use `argument`")
            return state
        state["status"] = "executable"
        state["effective_claim"] = row.get("claim", "")
        return state

    if status == "prose-only":
        if strength == "guaranteed":
            state["status"] = "invalid"
            state["problems"].append(
                f"row {rid}: a guaranteed row cannot be prose-only; either execute it or drop the claim"
            )
            return state
        if not state["owner"]:
            state["status"] = "invalid"
            state["problems"].append(f"row {rid}: prose-only needs an owner")
        if not state["debt"]:
            state["status"] = "invalid"
            state["problems"].append(f"row {rid}: prose-only needs a debt ticket")
        expires = parse_date(state["expires"])
        if expires is None:
            state["status"] = "invalid"
            state["problems"].append(f"row {rid}: prose-only needs an ISO expiry date")
        if state["status"] == "invalid":
            return state
        if expires < utc_today():
            state["status"] = "demoted"
            state["effective_claim"] = (
                f"{DEMOTED_PREFIX} — verifier expired {state['expires']}; "
                f"debt {state['debt']}"
            )
        else:
            state["status"] = "prose-only"
            state["effective_claim"] = (
                f"{PROSE_PREFIX} — executable evidence due {state['expires']}; "
                f"debt {state['debt']}"
            )
        return state

    state["status"] = "invalid"
    state["problems"].append(f"row {rid}: unknown demotion status {status!r}")
    return state


def check_structure(
    rows: dict[int, dict], policy: dict, root: Path = REPO_ROOT
) -> tuple[dict[int, dict], list[str]]:
    problems: list[str] = []
    states = {rid: row_state(rid, row, policy.get("rows", {}).get(str(rid))) for rid, row in rows.items()}

    for rid_s in policy.get("rows", {}):
        if int(rid_s) not in rows:
            problems.append(f"policy names row {rid_s}, which is not in guarantees.json")

    # M2: silence is not a claim. Every row in the production registry must be
    # classified (executable or prose-only with an expiry), so a new row cannot
    # slip into the table without a decision.
    missing = sorted(M2_REQUIRED - set(rows))
    if missing:
        problems.append(f"M2: rows missing from guarantees.json: {missing}")
    for rid in sorted(set(rows)):
        if policy.get("rows", {}).get(str(rid)) is None:
            problems.append(f"M2 row {rid} has no demotion policy entry")

    for rid, required in M1_REQUIRED.items():
        row = rows.get(rid)
        if row is None:
            problems.append(f"M1 row {rid} is missing from guarantees.json")
            continue
        pol = policy.get("rows", {}).get(str(rid))
        state = states[rid]
        if pol is None:
            problems.append(f"M1 row {rid} has no demotion policy entry")
            continue
        if state["status"] != "executable":
            problems.extend(state["problems"] or [f"M1 row {rid} is not executable"])
            continue
        kind = (state["verifier"] or {}).get("kind")
        if kind != required:
            problems.append(f"M1 row {rid} wants verifier `{required}`, got `{kind}`")

    for rid, state in states.items():
        problems.extend(state["problems"])
        if state["status"] != "executable":
            continue
        row = rows[rid]
        verifier = state["verifier"] or {}
        kind = verifier.get("kind")
        if kind == "corpus_pair" and not row.get("fixtures"):
            problems.append(f"row {rid}: corpus_pair has no fixtures")
        if kind == "run_gate":
            baseline = verifier.get("baseline")
            if not baseline:
                problems.append(f"row {rid}: run_gate has no baseline")
            elif not (root / baseline).exists():
                problems.append(f"row {rid}: baseline {baseline} does not exist")
        if kind in {"test_suite", "benchmark"}:
            command = verifier.get("command")
            if not command or not isinstance(command, list):
                problems.append(f"row {rid}: {kind} needs a command list")
            for req in verifier.get("requires") or []:
                path = root / req.get("path", "")
                if not path.exists():
                    problems.append(f"row {rid}: required invariant file {req.get('path')} does not exist")
                elif req.get("contains") and req["contains"] not in path.read_text():
                    problems.append(
                        f"row {rid}: {req.get('path')} does not contain {req['contains']!r}"
                    )
        if kind == "runtime_pin" and not verifier.get("expect"):
            problems.append(f"row {rid}: runtime_pin needs an expected panic string")

    if rows.get(16) and rows[16].get("strength") != "no-claim":
        problems.append("row 16 is the negative falsifier and must keep strength `no-claim`")
    return states, problems


def run(cmd: list[str], cwd: Path, timeout: int = 180) -> tuple[int, str]:
    proc = subprocess.run(
        cmd, cwd=str(cwd), capture_output=True, text=True, timeout=timeout
    )
    return proc.returncode, (proc.stdout or "") + (proc.stderr or "")


def run_corpus_pair(row: dict, hyb: str, root: Path) -> tuple[bool, str]:
    fixtures = row.get("fixtures") or []
    if not fixtures:
        return False, "no fixtures"
    for fixture in fixtures:
        path = fixture["path"]
        expect = fixture["expect"]
        if not (root / path).exists():
            return False, f"{path}: missing"
        code, out = run([hyb, "check", path], root)
        observed = "ACCEPT" if code == 0 else "REJECT"
        wanted = EXPECT_TO_OBSERVED.get(expect)
        if wanted is None:
            return False, f"{path}: unknown expect {expect!r}"
        if observed != wanted:
            return False, f"{path}: expected {wanted}, checker said {observed}"
        if expect == "accept-managed":
            deny, deny_out = run([hyb, "check", "--deny-managed", path], root)
            if deny == 0:
                return False, f"{path}: --deny-managed accepted a managed row"
            if "HYB-E8001" not in deny_out and "managed" not in deny_out.lower():
                return False, f"{path}: --deny-managed failed without a managed diagnostic"
    return True, f"{len(fixtures)} fixture(s) agree"


def run_run_gate(row: dict, verifier: dict, hyb: str, root: Path) -> tuple[bool, str]:
    baseline = verifier.get("baseline")
    fixtures = [f["path"] for f in row.get("fixtures") or []]
    if not baseline or not fixtures:
        return False, "run_gate needs a baseline and fixtures"
    gate = root / "tools" / "managed_gate.py"
    code, out = run([sys.executable, str(gate), hyb, baseline, *fixtures], root)
    if code != 0:
        return False, f"baseline run failed: {out.strip()[:200]}"

    with tempfile.TemporaryDirectory() as tmp:
        injected = Path(tmp) / Path(fixtures[0]).name
        injected.write_text((root / fixtures[0]).read_text())
        code, out = run([sys.executable, str(gate), hyb, baseline, str(injected)], root)
        if code == 0:
            return False, "an injected new managed block was not rejected"
        if "NEW MANAGED BLOCK" not in out:
            return False, f"injected block failed without the review diagnostic: {out.strip()[:200]}"
    return True, f"baseline {len(fixtures)} block(s) pass; injected block rejected"


RUNTIME_PIN_PROGRAM = """fn main() {
    ch := make(chan int, 1)
    close(ch)
    ch <- 1
}
"""


def run_runtime_pin(verifier: dict, hyb: str, go_bin: str, root: Path) -> tuple[bool, str]:
    expect = verifier.get("expect", "")
    if not expect:
        return False, "runtime_pin needs an expected panic string"
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        hy_prog = tmp_path / "pin.hy"
        hy_prog.write_text(RUNTIME_PIN_PROGRAM)
        code, out = run([hyb, "check", str(hy_prog)], root)
        if code != 0:
            return False, f"the checker now rejects the pin program (claim changed): {out.strip()[:200]}"
        out_base = tmp_path / "pin"
        code, out = run([hyb, "build", str(hy_prog), "-o", str(out_base), "--emit-go"], root)
        emitted = out_base.with_suffix(".go")
        if code != 0 or not emitted.exists():
            return False, f"emit failed: {out.strip()[:200]}"
        code, out = run([go_bin, "run", "-race", str(emitted)], tmp_path)
        if code == 0:
            return False, "the pinned program did not panic under -race"
        if expect not in out:
            return False, f"panic text did not contain {expect!r}: {out.strip()[:200]}"
    return True, f"checker accepts; go run -race: {expect}"


def run_test_suite(verifier: dict, hyb: str, go_bin: str, root: Path) -> tuple[bool, str]:
    command = verifier.get("command")
    if not command:
        return False, "test_suite needs a command"
    cwd = root / verifier.get("cwd", ".")
    timeout = int(verifier.get("timeout", 600))
    expect_exit = int(verifier.get("expect_exit", 0))
    code, out = run(substitute(command, hyb, go_bin), cwd, timeout)
    if code != expect_exit:
        return False, f"exit {code}, want {expect_exit}: {out.strip()[:200]}"
    needle = verifier.get("expect_output")
    if needle and needle not in out:
        return False, f"output did not contain {needle!r}: {out.strip()[:200]}"
    for req in verifier.get("requires") or []:
        path = root / req.get("path", "")
        if not path.exists():
            return False, f"required invariant file missing: {req.get('path')}"
        if req.get("contains") and req["contains"] not in path.read_text():
            return False, f"{req.get('path')} does not contain {req['contains']!r}"
    detail = f"`{' '.join(str(part) for part in command)}` exit {code}"
    if verifier.get("requires"):
        detail += f"; {len(verifier['requires'])} static invariant(s)"
    return True, detail


def run_benchmark(verifier: dict, hyb: str, go_bin: str, root: Path) -> tuple[bool, str]:
    command = verifier.get("command")
    expect = verifier.get("expect", "VERDICT: PASS")
    if not command:
        return False, "benchmark needs a command"
    cwd = root / verifier.get("cwd", ".")
    timeout = int(verifier.get("timeout", 600))
    code, out = run(substitute(command, hyb, go_bin), cwd, timeout)
    if code != 0:
        return False, f"bench exited {code}: {out.strip()[:200]}"
    if expect not in out:
        return False, f"bench output did not contain {expect!r}: {out.strip()[:200]}"
    match = re.search(r"fast path\s+([\d.]+)\s*ns\s*[≤<=]+\s*([\d.]+)\s*ns", out)
    if match:
        measured, budget = float(match.group(1)), float(match.group(2))
        ceiling = verifier.get("max_ns")
        if ceiling is not None and measured > float(ceiling):
            return False, f"measured {measured} ns exceeds max_ns {ceiling}"
        return True, f"bench verdict PASS; fast path {measured} ns ≤ {budget} ns"
    return True, f"bench verdict PASS ({expect})"


def execute_verifier(state: dict, row: dict, hyb: str, go_bin: str, root: Path) -> tuple[bool, str]:
    verifier = state["verifier"] or {}
    kind = verifier.get("kind")
    if kind == "corpus_pair":
        return run_corpus_pair(row, hyb, root)
    if kind == "run_gate":
        return run_run_gate(row, verifier, hyb, root)
    if kind == "runtime_pin":
        return run_runtime_pin(verifier, hyb, go_bin, root)
    if kind == "test_suite":
        return run_test_suite(verifier, hyb, go_bin, root)
    if kind == "benchmark":
        return run_benchmark(verifier, hyb, go_bin, root)
    return False, f"verifier kind {kind!r} is not executable"


def render(rows: dict[int, dict], states: dict[int, dict]) -> str:
    out = [
        "<!-- GENERATED by spikes/w2220-guarantees-as-code/guarantees_gate.py --render -->",
        "| Row | Strength | Verifier | State | Effective claim | Owner · review/debt |",
        "|-----|----------|----------|-------|-----------------|---------------------|",
    ]
    for rid in sorted(rows):
        row = rows[rid]
        state = states[rid]
        debt = state["debt"] or state["expires"] or state["review_by"] or "-"
        out.append(
            "| {} | {} | {} | {} | {} | {} · {} |".format(
                rid,
                row.get("strength", "-"),
                verifier_label(state["verifier"]),
                state["status"],
                state["effective_claim"].replace("|", "\\|"),
                state["owner"] or "-",
                debt,
            )
        )
    out.append("")
    return "\n".join(out)


def coverage_summary(states: dict[int, dict]) -> dict:
    counts = {"executable": 0, "prose-only": 0, "demoted": 0, "invalid": 0, "unregistered": 0}
    for state in states.values():
        counts[state["status"]] = counts.get(state["status"], 0) + 1
    not_executable = len(states) - counts["executable"]
    return {
        **counts,
        "total": len(states),
        "not_executable": not_executable,
        "wall": not_executable >= WALL_THRESHOLD,
    }


def render_coverage(rows: dict[int, dict], states: dict[int, dict], policy: dict) -> str:
    out = [
        "<!-- GENERATED by spikes/w2220-guarantees-as-code/guarantees_gate.py --render -->",
        "# W-2220 M2 coverage — the claims registry, applied to every row",
        "",
        COVERAGE_HEADER,
        "|-----|---------|----------|-------|----------|----------------------------------------|----------------------|",
    ]
    for rid in sorted(rows):
        row = rows[rid]
        state = states[rid]
        pol = policy.get("rows", {}).get(str(rid), {})
        reason = pol.get("reason") or (
            "executed by the gate" if state["status"] == "executable" else "-"
        )
        debt = state["debt"] or state["expires"] or state["review_by"] or "-"
        out.append(
            "| {} | {} | {} | {} | {} | {} | {} · {} |".format(
                rid,
                str(row.get("surface", "-")).replace("|", "\\|"),
                row.get("strength", "-"),
                state["status"],
                verifier_label(state["verifier"]).replace("|", "\\|"),
                reason.replace("|", "\\|"),
                state["owner"] or "-",
                debt,
            )
        )
    summary = coverage_summary(states)
    wall = "HIT — the approach is a documented wall" if summary["wall"] else "NOT HIT"
    out += [
        "",
        "## Summary",
        "",
        f"- executable: **{summary['executable']}/{summary['total']}**",
        f"- prose-only (owner + expiry + debt): **{summary['prose-only']}/{summary['total']}**",
        f"- demoted / invalid / unregistered: "
        f"**{summary['demoted'] + summary['invalid'] + summary['unregistered']}/{summary['total']}**",
        f"- pre-registered falsifier: **{summary['not_executable']} row(s) not executable** "
        f"(wall = {WALL_THRESHOLD} or more) — **{wall}**",
        "",
    ]
    return "\n".join(out)


def self_test(rows: dict[int, dict], policy: dict, hyb: str, go_bin: str, root: Path) -> int:
    cases: list[tuple[str, bool]] = []

    mutated = copy.deepcopy(rows)
    first = mutated[1]["fixtures"][0]
    first["expect"] = "accept" if first["expect"] == "reject" else "reject"
    ok, _ = run_corpus_pair(mutated[1], hyb, root)
    cases.append(("row 1: flipped fixture verdict is rejected", not ok))

    mutated_pol = copy.deepcopy(policy)
    del mutated_pol["rows"]["1"]
    _, problems = check_structure(rows, mutated_pol, root)
    cases.append(("policy drop: row 1 is not left unregistered", any("row 1" in p for p in problems)))

    mutated_pol = copy.deepcopy(policy)
    mutated_pol["rows"]["1"]["status"] = "prose-only"
    mutated_pol["rows"]["1"]["verifier"] = {"kind": "argument"}
    mutated_pol["rows"]["1"]["expires"] = "2099-01-01"
    mutated_pol["rows"]["1"]["debt"] = "SELFTEST"
    _, problems = check_structure(rows, mutated_pol, root)
    cases.append(("guaranteed prose-only: refused", any("guaranteed" in p for p in problems)))

    mutated_pol = copy.deepcopy(policy)
    mutated_pol["rows"]["16"]["verifier"]["expect"] = "no panic observed"
    state = row_state(16, rows[16], mutated_pol["rows"]["16"])
    ok, _ = run_runtime_pin(state["verifier"], hyb, go_bin, root)
    cases.append(("runtime pin: wrong panic text is rejected", not ok))

    mutated_pol = copy.deepcopy(policy)
    mutated_pol["rows"]["9"]["expires"] = "2020-01-01"
    state = row_state(9, rows[9], mutated_pol["rows"]["9"])
    text = render(rows, {rid: row_state(rid, row, mutated_pol["rows"].get(str(rid))) for rid, row in rows.items()})
    cases.append(("prose expiry: rendered row is demoted", state["status"] == "demoted" and DEMOTED_PREFIX in text))

    mutated_pol = copy.deepcopy(policy)
    mutated_pol["rows"]["18"]["verifier"]["command"] = [sys.executable, "-c", "import sys; sys.exit(1)"]
    state = row_state(18, rows[18], mutated_pol["rows"]["18"])
    ok, _ = run_test_suite(state["verifier"], hyb, go_bin, root)
    cases.append(("test_suite: a failing command is rejected", not ok))

    mutated_pol = copy.deepcopy(policy)
    mutated_pol["rows"]["20"]["verifier"]["expect"] = "VERDICT: IMPOSSIBLE"
    state = row_state(20, rows[20], mutated_pol["rows"]["20"])
    ok, _ = run_benchmark(state["verifier"], hyb, go_bin, root)
    cases.append(("benchmark: a wrong verdict is rejected", not ok))

    mutated_pol = copy.deepcopy(policy)
    del mutated_pol["rows"]["20"]
    _, problems = check_structure(rows, mutated_pol, root)
    cases.append(("M2 coverage: a dropped row is caught", any("row 20" in p for p in problems)))

    for name, caught in cases:
        print(("  PASS  " if caught else "  FAIL  ") + name)
    caught = sum(1 for _, ok in cases if ok)
    print(f"self-test: {caught}/{len(cases)} mutations caught")
    return 0 if caught == len(cases) else 1


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", default=str(REPO_ROOT), help="repository root")
    ap.add_argument("--claims", default="guarantees.json", help="claims registry (relative to root)")
    ap.add_argument(
        "--overlay",
        default=str(SPIKE_DIR / "m2_overrides.json"),
        help="M2 fixture overlay (relative to root; empty string disables it)",
    )
    ap.add_argument("--policy", default=str(SPIKE_DIR / "demotion_policy.json"))
    ap.add_argument("--rendered", default=str(SPIKE_DIR / "rendered_claims.md"))
    ap.add_argument("--coverage", default=str(SPIKE_DIR / "coverage.md"))
    ap.add_argument("--hyb", default=None)
    ap.add_argument("--go", default="go")
    ap.add_argument("--render", action="store_true", help="rewrite the rendered tables and exit")
    ap.add_argument("--self-test", action="store_true", help="run the seeded-mutation self-test")
    ap.add_argument("--timing", action="store_true", help="print per-row and total gate wall time")
    args = ap.parse_args(argv)

    root = Path(args.root).resolve()
    claims_path = root / args.claims
    if not claims_path.exists():
        print(f"GATE ERROR: claims registry not found: {claims_path}", file=sys.stderr)
        return 2
    hyb = args.hyb or str(root / "compiler" / "target" / "debug" / "hyb")
    if not Path(hyb).exists():
        print(f"GATE ERROR: checker not found: {hyb} (build with `cargo build -p hyb`)", file=sys.stderr)
        return 2

    try:
        claims, _overlay = load_claims(root, args.claims, args.overlay or None)
    except ValueError as exc:
        print(f"GATE ERROR: {exc}", file=sys.stderr)
        return 2
    rows = claim_rows(claims)
    policy = load_json(Path(args.policy))
    states, problems = check_structure(rows, policy, root)
    table = render(rows, states)
    coverage = render_coverage(rows, states, policy)
    summary = coverage_summary(states)

    if args.self_test:
        print("guarantees-as-code M2 self-test")
        problems.extend([f"structure: {p}" for p in problems])
        rc = self_test(rows, policy, hyb, args.go, root)
        if problems:
            for p in problems:
                print("  STRUCT " + p)
            return 1
        return rc

    if args.render:
        Path(args.rendered).write_text(table)
        Path(args.coverage).write_text(coverage)
        print(f"rendered {args.rendered}")
        print(f"rendered {args.coverage}")
        return 0 if not problems else 1

    started = time.perf_counter()
    for rid in sorted(states):
        state = states[rid]
        row = rows[rid]
        if state["status"] != "executable":
            print(f"  row {rid:<2} {verifier_label(state['verifier']):<34} {state['status'].upper()}")
            continue
        row_started = time.perf_counter()
        ok, detail = execute_verifier(state, row, hyb, args.go, root)
        elapsed = time.perf_counter() - row_started
        timing = f"  [{elapsed:.3f}s]" if args.timing else ""
        print(
            f"  row {rid:<2} {verifier_label(state['verifier']):<34} "
            f"{'OK' if ok else 'FAIL'}  {detail}{timing}"
        )
        if not ok:
            problems.append(f"row {rid}: {detail}")
    if args.timing:
        print(f"gate wall time: {time.perf_counter() - started:.3f}s")

    for label, path, expected in (
        ("rendered table", Path(args.rendered), table),
        ("coverage table", Path(args.coverage), coverage),
    ):
        try:
            committed = path.read_text()
        except FileNotFoundError:
            committed = None
        if committed != expected:
            problems.append(f"{label} drift: run --render and commit the result")

    if problems:
        for problem in problems:
            print("GATE: " + problem, file=sys.stderr)
        print("guarantees-as-code M2: FAIL")
        return 1
    print(
        "guarantees-as-code M2: PASS "
        f"({summary['executable']}/{summary['total']} executable, "
        f"{summary['prose-only']}/{summary['total']} prose-only; "
        f"wall {'HIT' if summary['wall'] else 'NOT HIT'})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
