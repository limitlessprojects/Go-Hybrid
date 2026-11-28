"""Tests for W-4729 directional ownership-breaking release witness synthesizer."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ownership_witness import (  # noqa: E402
    ApiOp,
    ApiRevision,
    OpEffect,
    WitnessStatus,
    check_hy_client,
    enumerate_breaking_sequences,
    find_shortest_breaking_witness,
)

TOOLCHAIN = "hyc-0.1.0-pinned"


def make_rev(version: str, submit_effect: OpEffect, inspect_effect: OpEffect = OpEffect.BORROW, locals_pair=("req", "res")) -> ApiRevision:
    return ApiRevision(
        version=version,
        toolchain=TOOLCHAIN,
        ops={
            "Inspect": ApiOp("Inspect", inspect_effect, body_local_names=(locals_pair[0],)),
            "Submit": ApiOp("Submit", submit_effect, body_local_names=(locals_pair[1],)),
        },
    )


class TestOwnershipWitness(unittest.TestCase):
    def test_borrow_to_consume_finds_shortest_witness_submit_inspect(self) -> None:
        old = make_rev("v1.0.0", OpEffect.BORROW)
        candidate = make_rev("v1.1.0", OpEffect.CONSUME)
        report = find_shortest_breaking_witness(old, candidate)

        self.assertEqual(report.status, WitnessStatus.BREAKING_CHANGE)
        self.assertEqual(report.witness_calls, ("Submit", "Inspect"))
        self.assertIsNotNone(report.old_check)
        self.assertTrue(report.old_check.passed)
        self.assertIsNotNone(report.candidate_check)
        self.assertFalse(report.candidate_check.passed)
        self.assertEqual(report.candidate_check.diagnostic.code, "HYB-OWN001")
        self.assertEqual(report.candidate_check.diagnostic.call_index, 1)
        self.assertEqual(report.candidate_check.diagnostic.op_name, "Inspect")
        self.assertIn("api.Submit(x)", report.hy_client_source)
        self.assertIn("api.Inspect(x)", report.hy_client_source)

    def test_exhaustive_enumeration_matches_127_and_114_model_counts(self) -> None:
        old = make_rev("v1.0.0", OpEffect.BORROW)
        candidate = make_rev("v1.1.0", OpEffect.CONSUME)

        total, breaking = enumerate_breaking_sequences(old, candidate, max_len=6)
        self.assertEqual(total, 127)
        self.assertEqual(breaking, 114)

        # Reverse direction (consume -> borrow) admits strictly more clients: 0 regressions.
        total_rev, breaking_rev = enumerate_breaking_sequences(candidate, old, max_len=6)
        self.assertEqual(total_rev, 127)
        self.assertEqual(breaking_rev, 0)

        # Unchanged behavior: 0 regressions.
        total_same, breaking_same = enumerate_breaking_sequences(old, old, max_len=6)
        self.assertEqual(total_same, 127)
        self.assertEqual(breaking_same, 0)

    def test_consume_to_borrow_is_non_breaking(self) -> None:
        old = make_rev("v1.0.0", OpEffect.CONSUME)
        candidate = make_rev("v1.1.0", OpEffect.BORROW)
        report = find_shortest_breaking_witness(old, candidate)

        self.assertEqual(report.status, WitnessStatus.NO_WITNESS)
        self.assertEqual(report.witness_calls, ())

    def test_unchanged_api_with_renamed_locals_gives_no_witness(self) -> None:
        old = make_rev("v1.0.0", OpEffect.BORROW, locals_pair=("tmpA", "bufA"))
        candidate = make_rev("v1.0.1", OpEffect.BORROW, locals_pair=("tmpRenamed", "bufRenamed"))
        report = find_shortest_breaking_witness(old, candidate)

        self.assertEqual(report.status, WitnessStatus.NO_WITNESS)
        self.assertEqual(report.witness_calls, ())

    def test_unsupported_aliasing_returns_unknown(self) -> None:
        old = make_rev("v1.0.0", OpEffect.BORROW)
        candidate = ApiRevision(
            version="v1.1.0",
            toolchain=TOOLCHAIN,
            ops={
                "Inspect": ApiOp("Inspect", OpEffect.BORROW),
                "Submit": ApiOp("Submit", OpEffect.UNKNOWN),
            },
        )
        report = find_shortest_breaking_witness(old, candidate)

        self.assertEqual(report.status, WitnessStatus.UNKNOWN)
        self.assertEqual(report.witness_calls, ())

    def test_unpinned_toolchain_returns_unknown(self) -> None:
        old = make_rev("v1.0.0", OpEffect.BORROW)
        candidate = ApiRevision(
            version="v1.1.0",
            toolchain="hyc-0.2.0-different",
            ops=make_rev("v1.1.0", OpEffect.CONSUME).ops,
        )
        report = find_shortest_breaking_witness(old, candidate)
        self.assertEqual(report.status, WitnessStatus.UNKNOWN)

    def test_both_consume_no_false_positive(self) -> None:
        old = make_rev("v1.0.0", OpEffect.CONSUME, OpEffect.CONSUME)
        candidate = make_rev("v1.1.0", OpEffect.CONSUME, OpEffect.CONSUME)
        report = find_shortest_breaking_witness(old, candidate)
        self.assertEqual(report.status, WitnessStatus.NO_WITNESS)

    def test_replay_direct_check_on_generated_hy_client(self) -> None:
        old = make_rev("v1.0.0", OpEffect.BORROW)
        candidate = make_rev("v1.1.0", OpEffect.CONSUME)
        report = find_shortest_breaking_witness(old, candidate)
        self.assertTrue(check_hy_client(report.hy_client_source, old).passed)
        self.assertFalse(check_hy_client(report.hy_client_source, candidate).passed)


if __name__ == "__main__":
    unittest.main()
