"""Unit and corpus verification tests for W-1427 (#1427): Write-Through Exclusive Map-Slot Loans."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

SPIKE_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SPIKE_DIR))

from write_through_slot_loans import (  # noqa: E402
    LoanStmt,
    audit_checker_corpus,
    build_class_a_programs,
    build_class_b_program,
    build_class_c_program,
    run_all_gates,
    simulate_deferred_o8,
    simulate_write_through_o8,
    verify_composability_matrix,
)


class TestW1427WriteThroughSlotLoans(unittest.TestCase):
    def test_01_hy_reproducer_files_exist_and_contain_loan_shapes(self) -> None:
        hy_dir = SPIKE_DIR / "hy"
        expected = {
            "01_class_a_causal_inversion.hy": ("n := &m[\"a\"]", "read_bal(m)", "after_last_use_trigger := n.bal"),
            "02_class_b_zero_trip_loop_drop.hy": ("n := &m[\"a\"]", "for i in 0..0", "task worker(move m, done)"),
            "03_class_c_interproc_loan_clobber.hy": ("p := &m[\"a\"]", "set_five_hundred(m)", "last_read := n.bal"),
        }
        for fname, snippets in expected.items():
            path = hy_dir / fname
            self.assertTrue(path.is_file(), f"Missing reproducer {path}")
            content = path.read_text(encoding="utf-8")
            for snippet in snippets:
                self.assertIn(snippet, content, f"Missing snippet {snippet!r} in {fname}")

    def test_02_p1_class_a_causal_inversion_eliminated_by_write_through(self) -> None:
        prog_no_read, prog_with_read = build_class_a_programs()
        def_no_read = simulate_deferred_o8(prog_no_read)
        def_with_read = simulate_deferred_o8(prog_with_read)

        # Deferred lowering suffers causal inversion when a trailing pure read is added
        self.assertEqual(def_no_read.observations["during_loan"], 99)
        self.assertEqual(def_with_read.observations["during_loan"], 10)
        self.assertEqual(def_with_read.observations["direct_read"], 10)
        self.assertEqual(def_with_read.observations["after_loan"], 99)
        self.assertFalse(def_with_read.statement_boundary_invariants_hold)

        # Write-through emitter produces 99 deterministically in both cases
        wt_no_read = simulate_write_through_o8(prog_no_read, enforce_exclusive_a3=False)
        wt_with_read = simulate_write_through_o8(prog_with_read, enforce_exclusive_a3=False)
        self.assertEqual(wt_no_read.observations["during_loan"], 99)
        self.assertEqual(wt_with_read.observations["during_loan"], 99)
        self.assertEqual(wt_with_read.observations["direct_read"], 99)
        self.assertTrue(wt_with_read.statement_boundary_invariants_hold)

    def test_03_p1_exclusive_a3_rejects_map_reads_inside_live_loan_window(self) -> None:
        _, prog_with_read = build_class_a_programs()
        wt_checked = simulate_write_through_o8(prog_with_read, enforce_exclusive_a3=True)
        self.assertFalse(wt_checked.checker_accepted)
        self.assertEqual(wt_checked.diagnostic_code, "HYB-E3102")

    def test_04_p2_class_b_zero_trip_loop_drops_store_back_on_deferred_but_preserves_on_write_through(self) -> None:
        prog_b = build_class_b_program()
        def_b = simulate_deferred_o8(prog_b)
        wt_b = simulate_write_through_o8(prog_b, enforce_exclusive_a3=False)

        # Deferred lowering hands flush into 0-trip `for` body via `branch_inherit` and drops 99 before `move m`
        self.assertEqual(def_b.observations["worker_saw_bal"], 10)
        self.assertEqual(def_b.spawned_worker_map, {"a": 10})
        self.assertFalse(def_b.statement_boundary_invariants_hold)

        # Write-through stores `m[hyKey_n] = n` immediately at `n.bal = 99` before the loop & task spawn
        self.assertEqual(wt_b.observations["worker_saw_bal"], 99)
        self.assertEqual(wt_b.spawned_worker_map, {"a": 99})
        self.assertTrue(wt_b.statement_boundary_invariants_hold)

    def test_05_p3_class_c_interproc_clobber_and_pure_read_store_back_eliminated(self) -> None:
        prog_c = build_class_c_program()
        def_c = simulate_deferred_o8(prog_c, enforce_interproc_slot_a3=False)
        wt_c_emit = simulate_write_through_o8(prog_c, enforce_exclusive_a3=False)
        wt_c_check = simulate_write_through_o8(prog_c, enforce_exclusive_a3=True)

        # Deferred lowering clobbers callee's 500 back to 99 on `last_read := n.bal`
        self.assertEqual(def_c.observations["after_call"], 500)
        self.assertEqual(def_c.observations["last_read"], 99)
        self.assertEqual(def_c.observations["final_bal"], 99)

        # Write-through emitter never emits a store-back on a pure read (`last_read := n.bal`)
        self.assertEqual(wt_c_emit.observations["after_call"], 500)
        self.assertEqual(wt_c_emit.observations["final_bal"], 500)
        self.assertEqual(wt_c_emit.store_back_count, 1)

        # Checker layer rejects interprocedural slot-loan mutation inside live window with HYB-E3102
        self.assertFalse(wt_c_check.checker_accepted)
        self.assertEqual(wt_c_check.diagnostic_code, "HYB-E3102")

    def test_06_read_only_slot_loan_emits_zero_store_backs_under_write_through(self) -> None:
        # Mirrors Corpus #23 (`23-a1c-mapslot-copy-at-spawn.hy`)
        read_only_prog = [
            LoanStmt("map_init", map_var="m", key="frontend", value=5),
            LoanStmt("loan_open", map_var="m", key="frontend", loan_var="svc"),
            LoanStmt("loan_read", loan_var="svc", obs_name="current"),
            LoanStmt("task_spawn", map_var="m", key="frontend", obs_name="worker_saw"),
        ]
        def_ro = simulate_deferred_o8(read_only_prog)
        wt_ro = simulate_write_through_o8(read_only_prog)

        self.assertEqual(def_ro.store_back_count, 1)  # Spurious store-back on read-only loan
        self.assertEqual(wt_ro.store_back_count, 0)   # Zero store-backs on read-only loan
        self.assertTrue(wt_ro.checker_accepted)
        self.assertEqual(wt_ro.observations["current"], 5)
        self.assertEqual(wt_ro.observations["worker_saw"], 5)

    def test_07_non_lexical_window_preserves_corpus_09_after_last_use_insert(self) -> None:
        # Mirrors Corpus #09 (`09-a3-map-slot-valid.hy`): insert after last_use in the same block
        corpus_09_prog = [
            LoanStmt("map_init", map_var="m", key="existing", value=1),
            LoanStmt("loan_open", map_var="m", key="existing", loan_var="svc"),
            LoanStmt("loan_mutate", loan_var="svc", value=3),
            LoanStmt("loan_mutate", loan_var="svc", value=3),
            LoanStmt("map_insert", map_var="m", key="fresh", value=1),
        ]
        wt_09 = simulate_write_through_o8(corpus_09_prog, enforce_exclusive_a3=True)
        self.assertTrue(wt_09.checker_accepted)
        self.assertEqual(wt_09.diagnostic_code, "OK")
        self.assertEqual(wt_09.store_back_count, 2)
        self.assertEqual(wt_09.final_map, {"existing": 3, "fresh": 1})
        self.assertTrue(wt_09.statement_boundary_invariants_hold)

    def test_08_p4_corpus_audit_zero_verdict_flips(self) -> None:
        report = audit_checker_corpus()
        self.assertEqual(report.total_corpus_files, 130)
        self.assertEqual(len(report.slot_loan_fixtures), 11)
        self.assertEqual(report.accept_count, 5)
        self.assertEqual(report.reject_count, 6)
        self.assertEqual(report.verdict_flips, 0)
        self.assertEqual(report.diagnostic_flips, 0)
        self.assertEqual(report.lexical_with_block_regressions, 1)

        by_name = {f.filename: f for f in report.slot_loan_fixtures}
        self.assertEqual(by_name["23-a1c-mapslot-copy-at-spawn.hy"].write_through_store_backs, 0)
        self.assertEqual(by_name["09-a3-map-slot-valid.hy"].write_through_store_backs, 2)
        self.assertTrue(by_name["09-a3-map-slot-valid.hy"].broken_by_lexical_with_block)

    def test_09_p4_orthogonal_composability_matrix_with_1432_1441_1645(self) -> None:
        matrix = verify_composability_matrix()
        self.assertEqual(len(matrix), 6)
        self.assertEqual(matrix["1441_non_lowered_callarg_rejected"].diagnostic_code, "HYB-E3104")
        self.assertEqual(matrix["1432_multireturn_rebind_rejected"].diagnostic_code, "HYB-E3104")
        self.assertTrue(matrix["1432_whole_value_assign_writes_through"].accepted)
        self.assertEqual(matrix["1432_whole_value_assign_writes_through"].emitted_store_backs, 1)
        self.assertEqual(matrix["1645_interproc_slot_loan_a3_rejected"].diagnostic_code, "HYB-E3102")
        self.assertEqual(matrix["1645_const_map_slot_loan_rejected"].diagnostic_code, "HYB-E3006")

    def test_10_results_md_and_all_gates_pass(self) -> None:
        gates = run_all_gates()
        self.assertEqual(gates, {"P1": True, "P2": True, "P3": True, "P4": True})

        results_md = SPIKE_DIR / "RESULTS.md"
        self.assertTrue(results_md.is_file())
        text = results_md.read_text(encoding="utf-8")
        for token in ("P1", "P2", "P3", "P4", "0 / 130", "#1432", "#1441", "#1645", "GUARANTEES.md"):
            self.assertIn(token, text)


if __name__ == "__main__":
    unittest.main()
