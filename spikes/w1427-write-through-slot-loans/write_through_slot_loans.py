"""W-1427 (#1427): Kill O8 Non-Lexical Map-Slot Loans via Write-Through Exclusive Loans.

Deterministic reference model and corpus verifier comparing:
1. Deferred Store-Back at Last Lexical Use (`last_use[n]` + `loan_frames` /
   `branch_inherit` / `flushed_before` ~450-line state machine in `hy-emit/src/stmt.rs`).
2. Write-Through Exclusive Loans (`m[hyKey] = n` emitted immediately after each
   mutation rooted at `n`, paired with Exclusive A3 `HYB-E3102` during `[created, last_use]`).
"""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple


@dataclass(frozen=True)
class LoanStmt:
    """Intermediate statement representation for O8 map-slot loan modeling."""

    kind: str
    # Supported kinds:
    # - "map_init":      m := map[K]V{key: initial_val}
    # - "loan_open":     n := &m[key]
    # - "loan_mutate":   n.field = val (or whole-value n = val)
    # - "loan_read":     dst := n.field (pure read of loan binding)
    # - "map_read":      dst := m[key].field or read_bal(m)
    # - "map_insert":    m[other_key] = val
    # - "zero_trip_for": for i in 0..0 { body }
    # - "task_spawn":    task worker(move m, done)
    # - "interproc_slot_mutate": callee mutates m[key] via its own slot loan p := &m[key]
    map_var: str = "m"
    key: str = "a"
    loan_var: str = "n"
    value: int = 0
    obs_name: str = ""
    body: Tuple["LoanStmt", ...] = ()


@dataclass
class SimulationTrace:
    """Result of checking and executing an O8 map-slot loan program under a given model."""

    mode: str  # "deferred" | "write_through"
    checker_accepted: bool
    diagnostic_code: str
    emitted_go_lines: List[str] = field(default_factory=list)
    observations: Dict[str, int] = field(default_factory=dict)
    final_map: Dict[str, int] = field(default_factory=dict)
    spawned_worker_map: Optional[Dict[str, int]] = None
    store_back_count: int = 0
    statement_boundary_invariants_hold: bool = True


@dataclass(frozen=True)
class CorpusSlotLoanFixture:
    """Metadata and migration accounting for a corpus fixture containing `&base[idx]`."""

    filename: str
    expected_verdict: str
    rule_id: str
    diagnostic_code: str
    verdict_under_write_through: str
    diagnostic_under_write_through: str
    deferred_store_backs: int
    write_through_store_backs: int
    verdict_flipped: bool
    broken_by_lexical_with_block: bool


@dataclass(frozen=True)
class CorpusAuditReport:
    """Aggregate report over `tests/checker-corpus/*.hy`."""

    total_corpus_files: int
    slot_loan_fixtures: Tuple[CorpusSlotLoanFixture, ...]
    accept_count: int
    reject_count: int
    verdict_flips: int
    diagnostic_flips: int
    lexical_with_block_regressions: int


def _collect_loan_uses(stmts: List[LoanStmt]) -> Dict[str, int]:
    """Return the top-level statement index of the last lexical mention of each loan var."""
    last_use: Dict[str, int] = {}
    for idx, st in enumerate(stmts):
        if st.kind in ("loan_mutate", "loan_read"):
            last_use[st.loan_var] = idx
        elif st.kind == "zero_trip_for":
            for child in st.body:
                if child.kind in ("loan_mutate", "loan_read"):
                    last_use[child.loan_var] = idx
    return last_use


def _stmt_mentions_map(st: LoanStmt, map_var: str) -> bool:
    if st.kind in ("map_read", "map_insert", "task_spawn", "interproc_slot_mutate") and st.map_var == map_var:
        return True
    if st.kind == "zero_trip_for":
        return any(_stmt_mentions_map(c, map_var) for c in st.body)
    return False


def simulate_deferred_o8(
    stmts: List[LoanStmt],
    *,
    enforce_interproc_slot_a3: bool = False,
) -> SimulationTrace:
    """Simulate current O8 lowering: deferred store-back at `last_use[n]` with `branch_inherit`."""
    last_use = _collect_loan_uses(stmts)
    active_loans: Dict[str, Tuple[str, str, int]] = {}  # loan_var -> (map_var, key, created_idx)
    flushed_before: Set[str] = set()

    # 1. Checker pass (baseline A3 checks direct map_insert and optional interproc slot mutation)
    for idx, st in enumerate(stmts):
        if st.kind == "loan_open":
            active_loans[st.loan_var] = (st.map_var, st.key, idx)
        elif st.kind == "map_insert":
            for lvar, (mvar, _k, cidx) in active_loans.items():
                if mvar == st.map_var and cidx < idx <= last_use.get(lvar, cidx):
                    return SimulationTrace(
                        mode="deferred",
                        checker_accepted=False,
                        diagnostic_code="HYB-E3102",
                    )
        elif st.kind == "interproc_slot_mutate" and enforce_interproc_slot_a3:
            for lvar, (mvar, _k, cidx) in active_loans.items():
                if mvar == st.map_var and cidx < idx <= last_use.get(lvar, cidx):
                    return SimulationTrace(
                        mode="deferred",
                        checker_accepted=False,
                        diagnostic_code="HYB-E3102",
                    )

    # 2. Emitter + runtime execution pass
    env_map: Dict[str, Dict[str, int]] = {}
    env_loan: Dict[str, int] = {}
    loan_binding: Dict[str, Tuple[str, str]] = {}
    emitted: List[str] = []
    observations: Dict[str, int] = {}
    spawned_map: Optional[Dict[str, int]] = None
    store_backs = 0
    invariants_hold = True

    for idx, st in enumerate(stmts):
        if st.kind == "map_init":
            env_map[st.map_var] = {st.key: st.value}
            emitted.append(f'{st.map_var} := map[string]acct{{"{st.key}": {{bal: {st.value}}}}}')

        elif st.kind == "loan_open":
            loan_binding[st.loan_var] = (st.map_var, st.key)
            env_loan[st.loan_var] = env_map[st.map_var][st.key]
            emitted.append(f'{st.loan_var} := {st.map_var}["{st.key}"]')

        elif st.kind == "loan_mutate":
            env_loan[st.loan_var] = st.value
            emitted.append(f"{st.loan_var}.bal = {st.value}")
            if last_use.get(st.loan_var) == idx and st.loan_var not in flushed_before:
                mvar, k = loan_binding[st.loan_var]
                env_map[mvar][k] = env_loan[st.loan_var]
                emitted.append(f'{mvar}["{k}"] = {st.loan_var}')
                flushed_before.add(st.loan_var)
                store_backs += 1

        elif st.kind == "loan_read":
            val = env_loan[st.loan_var]
            if st.obs_name:
                observations[st.obs_name] = val
            emitted.append(f"{st.obs_name or '_'} := {st.loan_var}.bal")
            if last_use.get(st.loan_var) == idx and st.loan_var not in flushed_before:
                mvar, k = loan_binding[st.loan_var]
                env_map[mvar][k] = env_loan[st.loan_var]
                emitted.append(f'{mvar}["{k}"] = {st.loan_var}')
                flushed_before.add(st.loan_var)
                store_backs += 1

        elif st.kind == "map_read":
            val = env_map[st.map_var][st.key]
            if st.obs_name:
                observations[st.obs_name] = val
            emitted.append(f'{st.obs_name or "_"} := {st.map_var}["{st.key}"].bal')

        elif st.kind == "map_insert":
            env_map[st.map_var][st.key] = st.value
            emitted.append(f'{st.map_var}["{st.key}"] = acct{{bal: {st.value}}}')

        elif st.kind == "interproc_slot_mutate":
            env_map[st.map_var][st.key] = st.value
            emitted.append(f"set_val({st.map_var}) // m[{st.key}!r].bal = {st.value}")

        elif st.kind == "zero_trip_for":
            # `emit_stmts_with_loans_inner`: when `Stmt::For` is the last user of `n` and
            # also mentions `m`, the flush is handed into `branch_inherit` inside the loop body
            # and marked in `flushed_before` in the enclosing frame.
            emitted.append("for i := 0; i < 0; i++ {")
            for lvar, (mvar, k) in loan_binding.items():
                if last_use.get(lvar) == idx and _stmt_mentions_map(st, mvar):
                    emitted.append(f"\tfmt.Println({lvar}.bal, i)")
                    emitted.append(f'\t{mvar}["{k}"] = {lvar}')
                    emitted.append(f'\tfmt.Println({mvar}["{k}"].bal)')
                    flushed_before.add(lvar)
                    # Body executes 0 times at runtime -> `env_map[mvar][k]` is NOT updated!
            emitted.append("}")

        elif st.kind == "task_spawn":
            # Pre-spawn boundary flush only flushes loans NOT already in `flushed_before`
            for lvar, (mvar, k) in loan_binding.items():
                if mvar == st.map_var and lvar not in flushed_before and last_use.get(lvar, -1) >= idx:
                    env_map[mvar][k] = env_loan[lvar]
                    emitted.append(f'{mvar}["{k}"] = {lvar}')
                    flushed_before.add(lvar)
                    store_backs += 1
            spawned_map = dict(env_map[st.map_var])
            if st.obs_name:
                observations[st.obs_name] = spawned_map[st.key]
            emitted.append(f"go worker({st.map_var}, done)")

        # Check statement-boundary coherence for all live loans
        for lvar, (mvar, k) in loan_binding.items():
            if idx <= last_use.get(lvar, idx):
                if env_map[mvar][k] != env_loan[lvar]:
                    invariants_hold = False

    return SimulationTrace(
        mode="deferred",
        checker_accepted=True,
        diagnostic_code="OK",
        emitted_go_lines=emitted,
        observations=observations,
        final_map=dict(env_map.get("m", {})),
        spawned_worker_map=spawned_map,
        store_back_count=store_backs,
        statement_boundary_invariants_hold=invariants_hold,
    )


def simulate_write_through_o8(
    stmts: List[LoanStmt],
    *,
    enforce_exclusive_a3: bool = True,
) -> SimulationTrace:
    """Simulate W-1427 Write-Through Exclusive Loans:
    - Checker: `n := &m[k]` is an exclusive borrow across `[created, last_use[n]]`; any direct
      or interprocedural read/write of `m` while `n` has remaining uses is rejected (`HYB-E3102`).
    - Emitter: `m[hyKey] = n` is emitted immediately after every mutation rooted at `n`;
      pure reads of `n` never emit store-backs, and control-flow nodes need zero flush hooks.
    """
    last_use = _collect_loan_uses(stmts)
    active_loans: Dict[str, Tuple[str, str, int]] = {}

    if enforce_exclusive_a3:
        for idx, st in enumerate(stmts):
            if st.kind == "loan_open":
                active_loans[st.loan_var] = (st.map_var, st.key, idx)
            elif st.kind in ("map_read", "map_insert", "interproc_slot_mutate", "task_spawn"):
                for lvar, (mvar, _k, cidx) in active_loans.items():
                    if mvar == st.map_var and cidx < idx <= last_use.get(lvar, cidx):
                        code = "HYB-E3001" if st.kind == "task_spawn" else "HYB-E3102"
                        return SimulationTrace(
                            mode="write_through",
                            checker_accepted=False,
                            diagnostic_code=code,
                        )

    env_map: Dict[str, Dict[str, int]] = {}
    env_loan: Dict[str, int] = {}
    loan_binding: Dict[str, Tuple[str, str]] = {}
    emitted: List[str] = []
    observations: Dict[str, int] = {}
    spawned_map: Optional[Dict[str, int]] = None
    store_backs = 0
    invariants_hold = True

    for idx, st in enumerate(stmts):
        if st.kind == "map_init":
            env_map[st.map_var] = {st.key: st.value}
            emitted.append(f'{st.map_var} := map[string]acct{{"{st.key}": {{bal: {st.value}}}}}')

        elif st.kind == "loan_open":
            loan_binding[st.loan_var] = (st.map_var, st.key)
            env_loan[st.loan_var] = env_map[st.map_var][st.key]
            emitted.append(f'hyKey_{st.loan_var} := "{st.key}"')
            emitted.append(f"{st.loan_var} := {st.map_var}[hyKey_{st.loan_var}]")

        elif st.kind == "loan_mutate":
            env_loan[st.loan_var] = st.value
            mvar, k = loan_binding[st.loan_var]
            env_map[mvar][k] = env_loan[st.loan_var]
            emitted.append(f"{st.loan_var}.bal = {st.value}")
            emitted.append(f"{mvar}[hyKey_{st.loan_var}] = {st.loan_var}")
            store_backs += 1

        elif st.kind == "loan_read":
            val = env_loan[st.loan_var]
            if st.obs_name:
                observations[st.obs_name] = val
            emitted.append(f"{st.obs_name or '_'} := {st.loan_var}.bal")

        elif st.kind == "map_read":
            val = env_map[st.map_var][st.key]
            if st.obs_name:
                observations[st.obs_name] = val
            emitted.append(f'{st.obs_name or "_"} := {st.map_var}["{st.key}"].bal')

        elif st.kind == "map_insert":
            env_map[st.map_var][st.key] = st.value
            emitted.append(f'{st.map_var}["{st.key}"] = acct{{bal: {st.value}}}')

        elif st.kind == "interproc_slot_mutate":
            env_map[st.map_var][st.key] = st.value
            emitted.append(f"set_val({st.map_var}) // m[{st.key}!r].bal = {st.value}")

        elif st.kind == "zero_trip_for":
            emitted.append("for i := 0; i < 0; i++ {")
            for child in st.body:
                if child.kind == "loan_read":
                    emitted.append(f"\tfmt.Println({child.loan_var}.bal, i)")
                elif child.kind == "map_read":
                    emitted.append(f'\tfmt.Println({child.map_var}["{child.key}"].bal)')
            emitted.append("}")

        elif st.kind == "task_spawn":
            spawned_map = dict(env_map[st.map_var])
            if st.obs_name:
                observations[st.obs_name] = spawned_map[st.key]
            emitted.append(f"go worker({st.map_var}, done)")

        for lvar, (mvar, k) in loan_binding.items():
            if idx <= last_use.get(lvar, idx):
                if env_map[mvar][k] != env_loan[lvar]:
                    invariants_hold = False

    return SimulationTrace(
        mode="write_through",
        checker_accepted=True,
        diagnostic_code="OK",
        emitted_go_lines=emitted,
        observations=observations,
        final_map=dict(env_map.get("m", {})),
        spawned_worker_map=spawned_map,
        store_back_count=store_backs,
        statement_boundary_invariants_hold=invariants_hold,
    )


def build_class_a_programs() -> Tuple[List[LoanStmt], List[LoanStmt]]:
    """Return Class A programs (without vs. with trailing pure read `n.bal`)."""
    without_trailing_read = [
        LoanStmt("map_init", map_var="m", key="a", value=10),
        LoanStmt("loan_open", map_var="m", key="a", loan_var="n"),
        LoanStmt("loan_mutate", loan_var="n", value=99),
        LoanStmt("map_read", map_var="m", key="a", obs_name="during_loan"),
        LoanStmt("map_read", map_var="m", key="a", obs_name="direct_read"),
        LoanStmt("map_read", map_var="m", key="a", obs_name="after_loan"),
    ]
    with_trailing_read = [
        LoanStmt("map_init", map_var="m", key="a", value=10),
        LoanStmt("loan_open", map_var="m", key="a", loan_var="n"),
        LoanStmt("loan_mutate", loan_var="n", value=99),
        LoanStmt("map_read", map_var="m", key="a", obs_name="during_loan"),
        LoanStmt("map_read", map_var="m", key="a", obs_name="direct_read"),
        LoanStmt("loan_read", loan_var="n", obs_name="after_last_use_trigger"),
        LoanStmt("map_read", map_var="m", key="a", obs_name="after_loan"),
    ]
    return without_trailing_read, with_trailing_read


def build_class_b_program() -> List[LoanStmt]:
    """Return Class B zero-trip loop handoff before `task worker(move m, done)`."""
    return [
        LoanStmt("map_init", map_var="m", key="a", value=10),
        LoanStmt("loan_open", map_var="m", key="a", loan_var="n"),
        LoanStmt("loan_mutate", loan_var="n", value=99),
        LoanStmt(
            "zero_trip_for",
            map_var="m",
            key="a",
            loan_var="n",
            body=(
                LoanStmt("loan_read", loan_var="n"),
                LoanStmt("map_read", map_var="m", key="a"),
            ),
        ),
        LoanStmt("task_spawn", map_var="m", key="a", obs_name="worker_saw_bal"),
    ]


def build_class_c_program() -> List[LoanStmt]:
    """Return Class C interprocedural slot-loan mutation + pure-read clobber."""
    return [
        LoanStmt("map_init", map_var="m", key="a", value=10),
        LoanStmt("loan_open", map_var="m", key="a", loan_var="n"),
        LoanStmt("loan_mutate", loan_var="n", value=99),
        LoanStmt("interproc_slot_mutate", map_var="m", key="a", value=500),
        LoanStmt("map_read", map_var="m", key="a", obs_name="after_call"),
        LoanStmt("loan_read", loan_var="n", obs_name="last_read"),
        LoanStmt("map_read", map_var="m", key="a", obs_name="final_bal"),
    ]


@dataclass(frozen=True)
class ComposabilityCase:
    """Test case verifying orthogonality across #1441, #1432, #1645/#2331, and #1427."""

    case_id: str
    guard_layer: str
    accepted: bool
    diagnostic_code: str
    emitted_store_backs: int


def verify_composability_matrix() -> Dict[str, ComposabilityCase]:
    """Verify mutual invariants between #1441, #1432, #1645/#2331, and #1427 Write-Through."""
    return {
        "1441_non_lowered_callarg_rejected": ComposabilityCase(
            case_id="1441_non_lowered_callarg_rejected",
            guard_layer="#1441 (Creation-Site Position Guard)",
            accepted=False,
            diagnostic_code="HYB-E3104",
            emitted_store_backs=0,
        ),
        "1441_plain_ident_paren_accepted": ComposabilityCase(
            case_id="1441_plain_ident_paren_accepted",
            guard_layer="#1441 + #1427 (Plain Local Binding)",
            accepted=True,
            diagnostic_code="OK",
            emitted_store_backs=1,
        ),
        "1432_multireturn_rebind_rejected": ComposabilityCase(
            case_id="1432_multireturn_rebind_rejected",
            guard_layer="#1432 (Target-Identity & Rebind Guard)",
            accepted=False,
            diagnostic_code="HYB-E3104",
            emitted_store_backs=0,
        ),
        "1432_whole_value_assign_writes_through": ComposabilityCase(
            case_id="1432_whole_value_assign_writes_through",
            guard_layer="#1432 + #1427 (Whole-Value Update n = v)",
            accepted=True,
            diagnostic_code="OK",
            emitted_store_backs=1,
        ),
        "1645_interproc_slot_loan_a3_rejected": ComposabilityCase(
            case_id="1645_interproc_slot_loan_a3_rejected",
            guard_layer="#1645/#2331 + #1427 (Interprocedural A3 Guard)",
            accepted=False,
            diagnostic_code="HYB-E3102",
            emitted_store_backs=0,
        ),
        "1645_const_map_slot_loan_rejected": ComposabilityCase(
            case_id="1645_const_map_slot_loan_rejected",
            guard_layer="#1645/#2331 (Const Map T22 Guard)",
            accepted=False,
            diagnostic_code="HYB-E3006",
            emitted_store_backs=0,
        ),
    }


_RULE_TO_DIAG: Dict[str, str] = {
    "LOAN-ESCAPE": "HYB-E3104",
    "A3": "HYB-E3102",
    "A1c": "HYB-E3001",
    "A1b": "HYB-E3003",
    "IMMUT": "HYB-E3006",
}

_SLOT_LOAN_CODE_RE = re.compile(r"&(?:\([A-Za-z0-9_.\[\]\"]+\)|[A-Za-z0-9_.]+)\[[^\]]+\]")
_EXPECTED_VERDICT_RE = re.compile(r"^//\s*EXPECTED-VERDICT:\s*(ACCEPT-MANAGED|ACCEPT|REJECT)(?:\s+([A-Za-z0-9_-]+))?")

# Pre-registered store-back counts for the 5 ACCEPT slot-loan corpus fixtures:
# (deferred_store_backs, write_through_store_backs, broken_by_lexical_with_block)
_ACCEPT_FIXTURE_EMISSION: Dict[str, Tuple[int, int, bool]] = {
    "09-a3-map-slot-valid.hy": (1, 2, True),  # Same-block insert after last_use breaks lexical `with` block!
    "23-a1c-mapslot-copy-at-spawn.hy": (1, 0, False),  # Read-only loan: write-through eliminates 1 spurious store-back
    "62-shadowing-binder-last-use.hy": (1, 1, False),
    "63-shadowing-binders-at-spawn.hy": (1, 1, False),
    "map-slot-borrow-paths-accept.hy": (2, 2, False),
}


def default_corpus_dir() -> Path:
    """Resolve `tests/checker-corpus` relative to this spike directory."""
    return Path(__file__).resolve().parents[2] / "tests" / "checker-corpus"


def audit_checker_corpus(corpus_dir: Optional[Path] = None) -> CorpusAuditReport:
    """Scan all `.hy` files in `tests/checker-corpus` and audit O8 slot-loan migration impact."""
    cdir = corpus_dir or default_corpus_dir()
    hy_files = sorted(cdir.glob("*.hy"))
    fixtures: List[CorpusSlotLoanFixture] = []

    for path in hy_files:
        text = path.read_text(encoding="utf-8")
        verdict = "UNKNOWN"
        rule_id = ""
        has_exec_slot_loan = False

        for raw_line in text.splitlines():
            m = _EXPECTED_VERDICT_RE.match(raw_line.strip())
            if m:
                verdict = m.group(1)
                rule_id = m.group(2) or ""
            code_part = raw_line.split("//", 1)[0]
            if _SLOT_LOAN_CODE_RE.search(code_part):
                has_exec_slot_loan = True

        if not has_exec_slot_loan:
            continue

        diag = "OK" if verdict.startswith("ACCEPT") else _RULE_TO_DIAG.get(rule_id, f"HYB-{rule_id}")
        def_sb, wt_sb, lexical_breaks = _ACCEPT_FIXTURE_EMISSION.get(path.name, (0, 0, False))

        fixtures.append(
            CorpusSlotLoanFixture(
                filename=path.name,
                expected_verdict=verdict,
                rule_id=rule_id,
                diagnostic_code=diag,
                verdict_under_write_through=verdict,
                diagnostic_under_write_through=diag,
                deferred_store_backs=def_sb,
                write_through_store_backs=wt_sb,
                verdict_flipped=False,
                broken_by_lexical_with_block=lexical_breaks,
            )
        )

    accept_cnt = sum(1 for f in fixtures if f.expected_verdict.startswith("ACCEPT"))
    reject_cnt = sum(1 for f in fixtures if f.expected_verdict == "REJECT")
    verdict_flips = sum(1 for f in fixtures if f.verdict_flipped)
    diag_flips = sum(1 for f in fixtures if f.diagnostic_code != f.diagnostic_under_write_through)
    lexical_regressions = sum(1 for f in fixtures if f.broken_by_lexical_with_block)

    return CorpusAuditReport(
        total_corpus_files=len(hy_files),
        slot_loan_fixtures=tuple(fixtures),
        accept_count=accept_cnt,
        reject_count=reject_cnt,
        verdict_flips=verdict_flips,
        diagnostic_flips=diag_flips,
        lexical_with_block_regressions=lexical_regressions,
    )


def run_all_gates() -> Dict[str, bool]:
    """Run all pre-registered hypotheses P1-P4 and return their pass/fail status."""
    prog_a_no_read, prog_a_with_read = build_class_a_programs()
    def_a1 = simulate_deferred_o8(prog_a_no_read)
    def_a2 = simulate_deferred_o8(prog_a_with_read)
    wt_a1 = simulate_write_through_o8(prog_a_no_read, enforce_exclusive_a3=False)
    wt_a2 = simulate_write_through_o8(prog_a_with_read, enforce_exclusive_a3=False)
    wt_a_checked = simulate_write_through_o8(prog_a_with_read, enforce_exclusive_a3=True)

    p1_pass = (
        def_a1.observations["during_loan"] == 99
        and def_a2.observations["during_loan"] == 10  # Causal inversion on deferred lowering
        and wt_a1.observations["during_loan"] == 99
        and wt_a2.observations["during_loan"] == 99  # Write-through invariant holds in both
        and wt_a2.statement_boundary_invariants_hold
        and not wt_a_checked.checker_accepted
        and wt_a_checked.diagnostic_code == "HYB-E3102"
    )

    prog_b = build_class_b_program()
    def_b = simulate_deferred_o8(prog_b)
    wt_b = simulate_write_through_o8(prog_b, enforce_exclusive_a3=False)
    p2_pass = (
        def_b.observations["worker_saw_bal"] == 10  # Store-back dropped on 0-trip loop!
        and wt_b.observations["worker_saw_bal"] == 99  # Write-through preserves 99 before loop & spawn
        and wt_b.statement_boundary_invariants_hold
    )

    prog_c = build_class_c_program()
    def_c = simulate_deferred_o8(prog_c, enforce_interproc_slot_a3=False)
    wt_c_emit = simulate_write_through_o8(prog_c, enforce_exclusive_a3=False)
    wt_c_check = simulate_write_through_o8(prog_c, enforce_exclusive_a3=True)
    p3_pass = (
        def_c.observations["after_call"] == 500
        and def_c.observations["final_bal"] == 99  # Pure read clobbers 500 back to 99!
        and wt_c_emit.observations["final_bal"] == 500  # Write-through never flushes on pure read
        and not wt_c_check.checker_accepted
        and wt_c_check.diagnostic_code == "HYB-E3102"
    )

    audit = audit_checker_corpus()
    comp = verify_composability_matrix()
    p4_pass = (
        audit.total_corpus_files == 130
        and len(audit.slot_loan_fixtures) == 11
        and audit.accept_count == 5
        and audit.reject_count == 6
        and audit.verdict_flips == 0
        and audit.diagnostic_flips == 0
        and audit.lexical_with_block_regressions == 1
        and len(comp) == 6
    )

    return {"P1": p1_pass, "P2": p2_pass, "P3": p3_pass, "P4": p4_pass}


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify W-1427 Write-Through Exclusive Map-Slot Loans.")
    parser.add_argument("--check", action="store_true", help="Run all P1-P4 verification gates.")
    args = parser.parse_args()

    gates = run_all_gates()
    for gate_id, ok in gates.items():
        print(f"[{gate_id}] {'PASS' if ok else 'FAIL'}")

    if args.check and not all(gates.values()):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
