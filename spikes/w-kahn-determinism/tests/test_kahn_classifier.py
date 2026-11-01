#!/usr/bin/env python3
"""Unit tests for the Proposal #2475 Kahn Determinism Classifier & Schedule Fuzzer."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

SPIKE_DIR = Path(__file__).resolve().parent.parent
if str(SPIKE_DIR) not in sys.path:
    sys.path.insert(0, str(SPIKE_DIR))

import kahn_classifier  # noqa: E402


class TestKahnClassifier(unittest.TestCase):
    def test_example04_without_timeout_is_admitted(self) -> None:
        src = (SPIKE_DIR / "hy" / "01-pipeline-kahn-admitted.hy").read_text(encoding="utf-8")
        res = kahn_classifier.classify_hy_source("01-pipeline-kahn-admitted.hy", src)
        self.assertTrue(res.kahn_deterministic)
        self.assertEqual(res.diagnostics, [])

    def test_example04_with_select_timeout_is_rejected_e3503_e3504(self) -> None:
        src = (SPIKE_DIR / "hy" / "02-pipeline-select-timeout-rejected.hy").read_text(encoding="utf-8")
        res = kahn_classifier.classify_hy_source("02-pipeline-select-timeout-rejected.hy", src)
        self.assertFalse(res.kahn_deterministic)
        codes = {d.code for d in res.diagnostics}
        self.assertIn("HYB-E3503", codes)
        self.assertIn("HYB-E3504", codes)

    def test_example03_worker_pool_is_rejected_and_marked_commutative_recoverable(self) -> None:
        src = (SPIKE_DIR / "hy" / "03-worker-pool-commutative-sink.hy").read_text(encoding="utf-8")
        res = kahn_classifier.classify_hy_source("03-worker-pool-commutative-sink.hy", src)
        self.assertFalse(res.kahn_deterministic)
        self.assertTrue(res.commutative_sink_recoverable)
        codes = {d.code for d in res.diagnostics}
        self.assertIn("HYB-E3501", codes)
        self.assertIn("HYB-E3502", codes)

    def test_cli_check_and_live_go_fuzzer(self) -> None:
        rc = kahn_classifier.main(["--check", "--live-go"])
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
