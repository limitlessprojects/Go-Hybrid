#!/usr/bin/env python3
"""Unit tests for tools/diag_warrants.py per Issue #4408."""

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from tools.diag_warrants import run_audit, load_registry, scan_warrants

class TestDiagWarrants(unittest.TestCase):
    def test_registry_contains_at_least_50_codes(self):
        reg = load_registry()
        self.assertGreaterEqual(len(reg), 50)
        self.assertIn("HYB-E4004", reg)
        self.assertIn("HYB-E4005", reg)

    def test_active_warrants_meet_m1_milestone(self):
        warrants = scan_warrants()
        self.assertGreaterEqual(len(warrants), 15)
        for code, paths in warrants.items():
            self.assertTrue(Path(ROOT / paths["reject"]).exists(), f"{code} missing reject.hy")
            self.assertTrue(Path(ROOT / paths["near_miss"]).exists(), f"{code} missing near_miss.hy")
            self.assertTrue(Path(ROOT / paths["warrant_toml"]).exists(), f"{code} missing warrant.toml")

    def test_audit_baseline_passes_cleanly(self):
        baseline_path = ROOT / "tools" / "diag_warrants_baseline.json"
        self.assertTrue(baseline_path.exists())
        report, code = run_audit(baseline_file=str(baseline_path), fail_on_orphan=True)
        self.assertEqual(code, 0)
        self.assertEqual(report["summary"]["new_orphans_count"], 0)

if __name__ == "__main__":
    unittest.main()
