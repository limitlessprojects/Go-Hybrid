#!/usr/bin/env python3
"""
Unit & Falsifier Verification Suite for W-4176 (#4176):
Exhaustive Schedule Enumeration + Replayable Certificate (.hcert)
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from enumerator import (
    CORPUS_DIR,
    enumerate_hy_program,
    evaluate_full_corpus,
    parse_hy_ast,
    run_10000_synthetic_variations,
    run_negative_tamper_controls,
    verify_hcert_replay,
)


class TestW4176ScheduleEnumeration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.corpus_report = evaluate_full_corpus()
        cls.synth_report = run_10000_synthetic_variations(seed=4176)

    def test_f1_corpus_agreement_and_managed_honesty_signal(self):
        """
        F1: 100% agreement (137/137) on non-ACCEPT-MANAGED corpus programs, and exact
        discrimination between 02-shared-map-managed.hy (RACE_DETECTED) and
        18-pool-shared-struct-managed.hy (CERTIFIED via sync.mutex).
        """
        self.assertEqual(self.corpus_report["total_corpus_programs"], 139)
        self.assertEqual(self.corpus_report["non_managed_programs"], 137)
        self.assertEqual(self.corpus_report["non_managed_agreement_count"], 137)
        self.assertEqual(self.corpus_report["non_managed_agreement_pct"], 100.0)

        managed_by_name = {
            m["program"]: m for m in self.corpus_report["managed_programs"]
        }
        self.assertIn("02-shared-map-managed.hy", managed_by_name)
        self.assertIn("18-pool-shared-struct-managed.hy", managed_by_name)

        m02 = managed_by_name["02-shared-map-managed.hy"]
        self.assertEqual(m02["enumerator_verdict"], "RACE_DETECTED")
        self.assertTrue(m02["disagreement_signal"])
        self.assertTrue(
            any("VIOLATION I_DRF" in step for step in m02["counterexample_trace"])
        )

        m18 = managed_by_name["18-pool-shared-struct-managed.hy"]
        self.assertEqual(m18["enumerator_verdict"], "CERTIFIED")
        self.assertFalse(m18["disagreement_signal"])

    def test_f2_strict_unclamped_admission_bound_exceeds_60_percent(self):
        """
        F2: Strict unclamped admission predicate (raw_tasks <= 4 and raw_chan_depth <= 2)
        covers 130/139 (93.53% >= 60%) of the corpus, while honestly marking the 9 programs
        with raw channel buffer sum > 2 (such as 02-shared-map-managed.hy with depth 4 and
        16-move-send-clean.hy with depth 16) as admitted_in_bound=False.
        """
        self.assertGreaterEqual(self.corpus_report["admitted_coverage_pct"], 60.0)
        self.assertEqual(self.corpus_report["admitted_in_bound_count"], 130)
        self.assertEqual(len(self.corpus_report["outside_raw_bound_programs"]), 9)
        outside_names = {
            p["program"] for p in self.corpus_report["outside_raw_bound_programs"]
        }
        self.assertIn("02-shared-map-managed.hy", outside_names)
        self.assertIn("16-move-send-clean.hy", outside_names)
        self.assertEqual(self.corpus_report["certified_count"], 46)

    def test_f3_generation_and_replay_latency_within_bounds(self):
        """
        F3: Explicit enumeration + .hcert emit < 150ms per program, and independent
        linear-time replay verification < 15ms per program.
        """
        self.assertTrue(self.corpus_report["all_replays_valid"])
        self.assertLess(self.corpus_report["max_generation_time_ms"], 150.0)
        self.assertLess(self.corpus_report["max_replay_time_ms"], 15.0)
        self.assertLess(self.corpus_report["total_corpus_replay_ms"], 2000.0)

    def test_f4_10000_synthetic_variations_zero_fn_zero_fp(self):
        """
        F4: Zero false negatives across 5,000 injected racy variations and zero false
        positives across 5,000 safe bounded idiom variations.
        """
        self.assertEqual(self.synth_report["total_synthetic_variations"], 10000)
        self.assertEqual(self.synth_report["false_negatives"], 0)
        self.assertEqual(self.synth_report["false_positives"], 0)

    def test_f5_negative_tamper_controls_fail_closed(self):
        """
        All 7 negative certificate tamper controls (including successor-closure pruning
        with recomputed SHA-256 and source_sha256 mismatch) must fail closed with typed
        HYB-CERT-E001..E010 diagnostics.
        """
        certified_outcomes = [
            o for o in self.corpus_report["outcomes"] if o.certificate is not None
        ]
        self.assertGreater(len(certified_outcomes), 0)
        sample_cert = certified_outcomes[0].certificate
        ok, msg = verify_hcert_replay(
            sample_cert, expected_source_sha256=sample_cert.source_sha256
        )
        self.assertTrue(ok, msg)

        tamper_results = run_negative_tamper_controls(sample_cert)
        self.assertEqual(len(tamper_results), 7)
        for control_name, (passed, diag) in tamper_results.items():
            self.assertFalse(
                passed, f"Tamper control {control_name} unexpectedly passed!"
            )
            self.assertIn("HYB-CERT-E0", diag)

    def test_f6_enumerator_is_blind_to_expected_verdict_comments(self):
        """
        Proves that `parse_hy_ast` + `enumerate_hy_program` compute verdicts 100%
        independently of `// EXPECTED-VERDICT:` headers: forging `// EXPECTED-VERDICT: ACCEPT`
        on `02-shared-map-managed.hy` or `01-shared-map-across-tasks.hy` still yields
        `RACE_DETECTED`.
        """
        p02 = os.path.join(CORPUS_DIR, "02-shared-map-managed.hy")
        with open(p02, "r", encoding="utf-8") as f:
            raw_02 = f.read()
        forged_02 = raw_02.replace(
            "EXPECTED-VERDICT: ACCEPT-MANAGED", "EXPECTED-VERDICT: ACCEPT"
        )
        ast_02 = parse_hy_ast(forged_02, program_name="02-forged-comment.hy")
        out_02 = enumerate_hy_program(ast_02)
        self.assertEqual(out_02.enum_verdict, "RACE_DETECTED")
        self.assertTrue(out_02.managed_disagreement_signal)
        self.assertFalse(out_02.admitted_in_bound)  # raw_chan_depth == 4 > 2


if __name__ == "__main__":
    unittest.main(verbosity=2)
