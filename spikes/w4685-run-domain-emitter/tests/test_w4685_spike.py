#!/usr/bin/env python3
"""Gates of wildcard proposal #4685: run-domain emit for the checked fragment."""

import os
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
import run_domain_emitter as m  # noqa: E402

PRED = m.BinOp("==", m.BinOp("%", m.BinOp("*", m.Var(), m.Var()), m.Const(5)), m.Const(1))


class TestArithmeticIdea(unittest.TestCase):
    """G6/G7 — the count must not depend on the representation."""

    def test_dense_and_run_domain_agree_on_every_sequence(self):
        seqs, bad, splits = m.check_equivalence(PRED)
        self.assertEqual(seqs, 3280)
        self.assertEqual(splits, 24604)
        self.assertEqual(bad, 0)

    def test_split_at_every_position_is_additive(self):
        # covered inside check_equivalence; pins the property on one long case
        seq = (1, 1, 0, 2, 1)
        whole = m.count_run_domain(m.compressed(seq), PRED)
        half = [m.count_run_domain(m.compressed(seq[:3]), PRED),
                m.count_run_domain(m.compressed(seq[3:]), PRED)]
        self.assertEqual(whole.total, sum(h.total for h in half))


class TestAdmission(unittest.TestCase):
    """G1-G5 — what must be rejected, and with which code."""

    def test_zero_length_rejected(self):
        res = m.count_run_domain([m.Run(1, 2), m.Run(1, 0)], PRED)
        self.assertEqual((res.kind, res.code), ("failure", m.E_ZERO_LENGTH))

    def test_logical_length_overflow_rejected_even_when_predicate_false(self):
        false_pred = m.Const(0)
        res = m.count_run_domain([m.Run(4, m.UINT64_MAX), m.Run(4, 1)], false_pred)
        self.assertEqual((res.kind, res.code), ("failure", m.E_LENGTH_OVERFLOW))

    def test_single_length_overflow_rejected(self):
        res = m.count_run_domain([m.Run(1, 1 << 64)], PRED)
        self.assertEqual((res.kind, res.code), ("failure", m.E_LENGTH_OVERFLOW))

    def test_sample_outside_int64_rejected(self):
        res = m.count_run_domain([m.Run(1 << 63, 1)], PRED)
        self.assertEqual((res.kind, res.code), ("failure", m.E_SAMPLE_RANGE))

    def test_time_read_rejected(self):
        res = m.count_run_domain([m.Run(1, 1)], m.BinOp("==", m.Time(), m.Const(0)))
        self.assertEqual((res.kind, res.code), ("failure", m.E_TIME_READ))

    def test_position_predicate_rejected(self):
        res = m.count_run_domain([m.Run(1, 1)], m.BinOp("<",
                                 m.BinOp("%", m.Index(), m.Const(2)), m.Const(1)))
        self.assertEqual((res.kind, res.code), ("failure", m.E_POSITION_READ))

    def test_modulo_by_zero_run_fails_at_runtime_not_silently(self):
        res = m.count_run_domain([m.Run(1, 1), m.Run(0, 1)],
                                 m.BinOp("%", m.Const(7), m.Var()))
        self.assertEqual((res.kind, res.code), ("failure", m.E_UNSUPPORTED))

    def test_int64_wraparound_matches_go_semantics(self):
        big = m.Run(m.INT64_MAX, 1)
        pred = m.BinOp("<", m.BinOp("+", m.Var(), m.Const(1)), m.Const(0))
        self.assertEqual(m.count_run_domain([big], pred).total, 1)
        self.assertEqual(m.dense_reference([big], pred), 1)


class TestNoExpansion(unittest.TestCase):
    """G9 — one evaluation per run, allocation following the run count."""

    def test_single_billion_run_needs_one_evaluation(self):
        res = m.count_run_domain([m.Run(1, 1_000_000_000)], PRED)
        self.assertEqual((res.kind, res.total), ("accepted", 1_000_000_000))
        self.assertEqual(res.predicate_evals, 1)
        self.assertEqual(res.run_alloc, 1)
        self.assertEqual(res.dense_alloc, 1_000_000_000)

    def test_alternating_run_has_no_advantage(self):
        res = m.count_run_domain([m.Run(1, 1)] * 4, PRED)
        self.assertEqual(res.predicate_evals, res.dense_alloc)


@unittest.skipIf(shutil.which("go") is None, "go toolchain not installed")
class TestEmittedGo(unittest.TestCase):
    """G8 — the emitted loop is real Go, compiles, and counts like the model."""

    def test_emitted_loop_compiles_and_matches_dense_reference(self):
        runs = [m.Run(1, 3), m.Run(0, 2), m.Run(2, 1), m.Run(1, 1)]
        src = m.emit_go(runs, PRED)
        with tempfile.TemporaryDirectory() as tmp:
            open(os.path.join(tmp, "main.go"), "w").write(src)
            open(os.path.join(tmp, "go.mod"), "w").write("module spike4685\n\ngo 1.21\n")
            unformatted = subprocess.run(["gofmt", "-l", "."], cwd=tmp, check=True,
                                     capture_output=True, text=True).stdout.strip()
            self.assertEqual(unformatted, "", "emitted Go is not gofmt-clean")
            out = subprocess.run(["go", "run", "."], cwd=tmp, check=True,
                                 capture_output=True, text=True).stdout.split()
        total, evals = int(out[0]), int(out[1])
        self.assertEqual(total, m.dense_reference(runs, PRED))
        self.assertEqual(evals, len(runs))
        self.assertEqual(total, 4)


if __name__ == "__main__":
    unittest.main()
