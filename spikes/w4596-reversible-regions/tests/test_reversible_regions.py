"""Tests for Wildcard Spike W-4596: Reversible Checked Regions."""

from __future__ import annotations

from pathlib import Path
import sys
import unittest

SPIKE_DIR = Path(__file__).resolve().parents[1]
if str(SPIKE_DIR) not in sys.path:
    sys.path.insert(0, str(SPIKE_DIR))

from reversible_region import (
    BOUNDARY_U64,
    MASK64,
    AddAssign,
    BinExpr,
    Const,
    RevIf,
    ReversibleRegion,
    Var,
    build_negative_corpus,
    build_positive_corpus,
    check_region,
    emit_go_pair,
    modinv64,
    rotl64,
    rotr64,
    run_1mib_roi_benchmark,
    synthesize_inverse,
    verify_program_roundtrip,
)


class TestReversibleCheckedRegions(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.positives = build_positive_corpus()
        cls.negatives = build_negative_corpus()
        cls.registry = {r.name: r for r in cls.positives}

    def test_corpus_sizes_preregistered(self) -> None:
        self.assertEqual(len(self.positives), 10)
        self.assertEqual(len(self.negatives), 10)

    def test_primitive_modular_and_rotation_inverses(self) -> None:
        for v in BOUNDARY_U64:
            for k in (0, 1, 7, 13, 31, 63, 64, 71):
                self.assertEqual(rotr64(rotl64(v, k), k), v)
                self.assertEqual(rotl64(rotr64(v, k), k), v)
        for c in (1, 3, 5, 0x9E3779B97F4A7C15, 0xBF58476D1CE4E5B9, MASK64):
            inv = modinv64(c)
            self.assertEqual((c * inv) & MASK64, 1)
        with self.assertRaises(ValueError):
            modinv64(2)
        with self.assertRaises(ValueError):
            modinv64(0)

    def test_all_10_positives_admitted_and_emit_dual_go(self) -> None:
        for region in self.positives:
            diags = check_region(region, self.registry)
            self.assertEqual(diags, [], f"{region.name} should be admitted")
            inv_ast = synthesize_inverse(region, self.registry)
            self.assertGreater(len(inv_ast), 0)
            go_src = emit_go_pair(region, self.registry)
            self.assertIn(f"func {region.name}Forward(s *{region.name}State)", go_src)
            self.assertIn(f"func {region.name}Inverse(s *{region.name}State)", go_src)

    def test_all_10_negatives_rejected_with_exact_diagnostic(self) -> None:
        seen_codes = set()
        for region, expected_code in self.negatives:
            diags = check_region(region, self.registry)
            codes = [d.code for d in diags]
            self.assertIn(expected_code, codes, f"{region.name}: expected {expected_code}, got {codes}")
            with self.assertRaises(ValueError):
                synthesize_inverse(region, self.registry)
            seen_codes.add(expected_code)
        self.assertEqual(len(seen_codes), 10)

    def test_four_required_isolation_attacks_fail_closed(self) -> None:
        by_name = {r.name: (r, code) for r, code in self.negatives}
        expected_isolation = {
            "N04_MutableSliceAliasing": "HYB-REV004",
            "N06_ChannelSendEscape": "HYB-REV006",
            "N08_IndirectForeignCall": "HYB-REV008",
            "N07_TaskSpawnCapture": "HYB-REV007",
        }
        for name, expected_code in expected_isolation.items():
            region, _ = by_name[name]
            diags = check_region(region, self.registry)
            self.assertTrue(any(d.code == expected_code for d in diags), f"Isolation attack {name} did not fail with {expected_code}")

    def test_roundtrip_100k_seeded_records_per_admitted_program(self) -> None:
        """Issue #4596 Requirement 2: >= 100,000 seeded random records + boundaries for EVERY admitted program."""
        total_records = 0
        for region in self.positives:
            res = verify_program_roundtrip(
                region,
                self.registry,
                num_random=100_000,
                seed=4596,
                reference_sample=64,
            )
            self.assertEqual(res["mismatches"], 0, f"Byte mismatch detected in {region.name}")
            total_records += res["random_records"]
        self.assertEqual(total_records, 1_000_000)

    def test_branch_discriminator_preservation_vs_mutation(self) -> None:
        bad = ReversibleRegion(
            name="BadBranchClobber",
            description="Clobbers discriminator y in else branch",
            hy_source="",
            mutable_scalars=("x", "y"),
            immutable_scalars=(),
            mutable_slices=(),
            body=(
                RevIf(
                    BinExpr("==", Var("y"), Const(0)),
                    (AddAssign("x", Const(1)),),
                    (AddAssign("y", Const(1)),),
                ),
            ),
        )
        diags = check_region(bad, self.registry)
        self.assertEqual([d.code for d in diags], ["HYB-REV005"])

    def test_roi_1mib_transform_and_abort_memory_advantage(self) -> None:
        """Issue #4596 Requirement 4: 1 MiB transform-and-abort comparison."""
        roi = run_1mib_roi_benchmark(num_words=131_072, seed=4596)
        self.assertEqual(roi["payload_bytes"], 1_048_576)
        self.assertTrue(roi["reversible_inverse"]["exact_restore"])
        self.assertTrue(roi["snapshot_copy"]["exact_restore"])
        self.assertTrue(roi["undo_log"]["exact_restore"])

        # Reversible inverse allocates orders of magnitude less extra memory than 1 MiB snapshot or undo log
        self.assertLess(roi["reversible_inverse"]["peak_extra_bytes"], 8_192)
        self.assertGreaterEqual(roi["snapshot_copy"]["peak_extra_bytes"], 1_048_576)
        self.assertGreaterEqual(roi["undo_log"]["peak_extra_bytes"], 1_500_000)


if __name__ == "__main__":
    unittest.main()
