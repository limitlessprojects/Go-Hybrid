#!/usr/bin/env python3
"""guarantees_gate.py — W-2220 M1: execute the claims table, not just render it.

`tools/check_guarantees.py` (issue #710) made `guarantees.json` the source of
the GUARANTEES.md table and `tools/check_claims_links.py` (#2221) verified its
citations. This spike adds the missing half of the W-2220 proposal:

1. **Verifier kinds.** A row's evidence is a typed verifier that the gate
   executes: `corpus_pair` (run the checker on the accept/reject pair),
   `run_gate` (run `tools/managed_gate.py`, including a new-block rejection
   probe), and `runtime_pin` (build the emitted Go and pin a runtime panic
   under `-race`).
2. **Demotion state machine.** A row is `executable` only while its verifier
   passes. `prose-only` requires an owner, an expiry, and a debt ticket; an
   expired row renders as **demoted to no claim**, never as prose that still
   reads like a guarantee.
3. **Seeded-mutation self-test.** `--self-test` mutates the registry/policy in
   five ways and fails unless every mutation is caught. A gate that cannot
   fail is not a gate.

M1 scope is rows 1, 8 and 16 (guaranteed / managed gate / negative falsifier).
Rows without a policy entry render as `unregistered (M1 out of scope)` rather
than silently inheriting a claim.

Usage:
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --render
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test
    python3 spikes/w2220-guarantees-as-code/guarantees_gate.py \
        --hyb compiler/target/debug/hyb --go go

Exit codes: 0 = every executable row verified and the rendered table matches;
1 = a violation or a caught self-test mutation; 2 = CLI misuse.
"""

from __future__ import annotations

import argparse
import copy
import datetime
import json
import subprocess
import sys
import tempfile
from pathlib import Path

SPIKE_DIR = Path(__file__).resolve().parent
REPO_ROOT = SPIKE_DIR.parents[1]

M1_REQUIRED = {1: "corpus_pair", 8: "run_gate", 16: "runtime_pin"}
ALLOWED_KINDS = {"corpus_pair", "run_gate", "runtime_pin", "argument"}
EXECUTABLE_KINDS = {"corpus_pair", "run_gate", "runtime_pin"}
EXPECT_TO_OBSERVED = {"reject": "REJECT", "accept": "ACCEPT", "accept-managed": "ACCEPT"}

PROSE_PREFIX = "**No claim (prose-only)**"
DEMOTED_PREFIX = "**Demoted to no claim**"


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


def verifier_label(verifier: dict | None) -> str:
    if not verifier:
        return "-"
    kind = verifier.get("kind", "?")
    extra = []
    for key in ("gate", "runtime"):
        if verifier.get(key):
            extra.append(str(verifier[key]))
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
        "effective_claim": "unregistered (M1 out of scope)",
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


def check_structure(rows: dict[int, dict], policy: dict) -> tuple[dict[int, dict], list[str]]:
    problems: list[str] = []
    states = {rid: row_state(rid, row, policy.get("rows", {}).get(str(rid))) for rid, row in rows.items()}

    for rid_s in policy.get("rows", {}):
        if int(rid_s) not in rows:
            problems.append(f"policy names row {rid_s}, which is not in guarantees.json")

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
        if kind == "corpus_pair" and not row.get("fixtures"):
            problems.append(f"M1 row {rid}: corpus_pair has no fixtures")
        if kind == "run_gate":
            baseline = (state["verifier"] or {}).get("baseline")
            if not baseline:
                problems.append(f"M1 row {rid}: run_gate has no baseline")
            elif not (REPO_ROOT / baseline).exists():
                problems.append(f"M1 row {rid}: baseline {baseline} does not exist")

    for rid, state in states.items():
        problems.extend(state["problems"])

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


def execute_verifier(state: dict, row: dict, hyb: str, go_bin: str, root: Path) -> tuple[bool, str]:
    verifier = state["verifier"] or {}
    kind = verifier.get("kind")
    if kind == "corpus_pair":
        return run_corpus_pair(row, hyb, root)
    if kind == "run_gate":
        return run_run_gate(row, verifier, hyb, root)
    if kind == "runtime_pin":
        return run_runtime_pin(verifier, hyb, go_bin, root)
    return False, f"verifier kind {kind!r} is not executable in M1"


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


def self_test(rows: dict[int, dict], policy: dict, hyb: str, go_bin: str, root: Path) -> int:
    cases: list[tuple[str, bool]] = []

    mutated = copy.deepcopy(rows)
    first = mutated[1]["fixtures"][0]
    first["expect"] = "accept" if first["expect"] == "reject" else "reject"
    ok, _ = run_corpus_pair(mutated[1], hyb, root)
    cases.append(("row 1: flipped fixture verdict is rejected", not ok))

    mutated_pol = copy.deepcopy(policy)
    del mutated_pol["rows"]["1"]
    _, problems = check_structure(rows, mutated_pol)
    cases.append(("policy drop: row 1 is not left unregistered", any("row 1" in p for p in problems)))

    mutated_pol = copy.deepcopy(policy)
    mutated_pol["rows"]["1"]["status"] = "prose-only"
    mutated_pol["rows"]["1"]["verifier"] = {"kind": "argument"}
    mutated_pol["rows"]["1"]["expires"] = "2099-01-01"
    mutated_pol["rows"]["1"]["debt"] = "SELFTEST"
    _, problems = check_structure(rows, mutated_pol)
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

    for name, caught in cases:
        print(("  PASS  " if caught else "  FAIL  ") + name)
    caught = sum(1 for _, ok in cases if ok)
    print(f"self-test: {caught}/{len(cases)} mutations caught")
    return 0 if caught == len(cases) else 1


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", default=str(REPO_ROOT), help="repository root")
    ap.add_argument("--claims", default="guarantees.json", help="claims registry (relative to root)")
    ap.add_argument("--policy", default=str(SPIKE_DIR / "demotion_policy.json"))
    ap.add_argument("--rendered", default=str(SPIKE_DIR / "rendered_claims.md"))
    ap.add_argument("--hyb", default=None)
    ap.add_argument("--go", default="go")
    ap.add_argument("--render", action="store_true", help="rewrite the rendered table and exit")
    ap.add_argument("--self-test", action="store_true", help="run the seeded-mutation self-test")
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

    claims = load_json(claims_path)
    rows = claim_rows(claims)
    policy = load_json(Path(args.policy))
    states, problems = check_structure(rows, policy)
    table = render(rows, states)

    if args.self_test:
        print("guarantees-as-code M1 self-test")
        problems.extend([f"structure: {p}" for p in problems])
        rc = self_test(rows, policy, hyb, args.go, root)
        if problems:
            for p in problems:
                print("  STRUCT " + p)
            return 1
        return rc

    if args.render:
        Path(args.rendered).write_text(table)
        print(f"rendered {args.rendered}")
        return 0 if not problems else 1

    for rid in sorted(states):
        state = states[rid]
        row = rows[rid]
        if state["status"] != "executable":
            print(f"  row {rid:<2} {verifier_label(state['verifier']):<34} {state['status'].upper()}")
            continue
        ok, detail = execute_verifier(state, row, hyb, args.go, root)
        print(f"  row {rid:<2} {verifier_label(state['verifier']):<34} {'OK' if ok else 'FAIL'}  {detail}")
        if not ok:
            problems.append(f"row {rid}: {detail}")

    try:
        committed = Path(args.rendered).read_text()
    except FileNotFoundError:
        committed = None
    if committed != table:
        problems.append("rendered table drift: run --render and commit the result")

    if problems:
        for problem in problems:
            print("GATE: " + problem, file=sys.stderr)
        print("guarantees-as-code M1: FAIL")
        return 1
    print("guarantees-as-code M1: PASS (all executable rows verified; rendered table matches)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
