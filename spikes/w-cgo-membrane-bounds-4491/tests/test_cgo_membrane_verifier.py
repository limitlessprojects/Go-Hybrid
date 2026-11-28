#!/usr/bin/env python3
"""
Hermetic unit tests for Issue #4491 Cgo Membrane Verifier.
"""

import unittest
import sys
from pathlib import Path

# Add spike directory to path
spike_dir = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(spike_dir))

import cgo_membrane_verifier

class TestCgoMembraneVerifier(unittest.TestCase):
    def test_conformance_matrix_size(self):
        results = cgo_membrane_verifier.run_conformance()
        self.assertEqual(len(results), 20, "Must have exactly 20 conformance patterns")

    def test_all_patterns_pass(self):
        results = cgo_membrane_verifier.run_conformance()
        for r in results:
            self.assertEqual(r["status"], "PASS", f"Pattern {r['pattern']} failed")

    def test_rejected_patterns_have_diagnostic_codes(self):
        results = cgo_membrane_verifier.run_conformance()
        rejected = [r for r in results if r["expected"] == "reject"]
        self.assertGreater(len(rejected), 0, "Must test rejection patterns")
        for r in rejected:
            self.assertTrue(r["rule"].startswith("HYB-E"), f"Rule {r['rule']} is not a valid HYB-E diagnostic")

    def test_live_go_suite(self):
        work_dir = spike_dir / "work"
        if (work_dir / "go.mod").exists():
            success, output = cgo_membrane_verifier.run_live_go(work_dir)
            self.assertTrue(success, f"Live Go verification failed:\n{output}")

if __name__ == "__main__":
    unittest.main()
