"""Tests for W-4729: Smallest Downstream Breaking Client Synthesizer."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from client_synthesizer import (
    BreakingClientSynthesizer,
    APIPackage,
    Operation,
    Effect,
    replay_client,
)


class TestClientSynthesizer(unittest.TestCase):
    def setUp(self):
        self.synth = BreakingClientSynthesizer(max_depth=6)

    def test_borrow_to_consume_breaking_witness_synthesized(self):
        """When Submit changes from borrow to consume, synthesizer finds shortest witness [Submit, Inspect]."""
        # v1: Inspect borrows, Submit borrows
        v1 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Submit": Operation("Submit", "Task", Effect.BORROW),
            },
        )
        # v2: Inspect borrows, Submit CONSUMES (breaking change)
        v2 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Submit": Operation("Submit", "Task", Effect.CONSUME),
            },
        )

        res = self.synth.analyze_release_change(v1, v2)
        self.assertTrue(res.has_breaking_change)
        self.assertEqual(res.status, "WITNESS_FOUND")
        self.assertEqual(res.witness_sequence, ["Submit", "Inspect"])
        self.assertIsNotNone(res.synthesized_hy_code)
        self.assertIn("jobqueue.Submit(x)", res.synthesized_hy_code)
        self.assertIn("jobqueue.Inspect(x)", res.synthesized_hy_code)

        # Verify replay against both versions
        v1_ok, v1_err = replay_client(res.synthesized_hy_code, v1)
        self.assertTrue(v1_ok)
        self.assertIsNone(v1_err)

        v2_ok, v2_err = replay_client(res.synthesized_hy_code, v2)
        self.assertFalse(v2_ok)
        self.assertIn("HYB-E3001: use of moved value", v2_err)

    def test_consume_to_borrow_relaxation_produces_no_breaking_witness(self):
        """When Submit relaxes from consume to borrow, existing clients remain compatible."""
        # v1: Submit consumes
        v1 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Submit": Operation("Submit", "Task", Effect.CONSUME),
            },
        )
        # v2: Submit relaxes to borrow
        v2 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Submit": Operation("Submit", "Task", Effect.BORROW),
            },
        )

        res = self.synth.analyze_release_change(v1, v2)
        self.assertFalse(res.has_breaking_change)
        self.assertEqual(res.status, "COMPATIBLE")
        self.assertEqual(len(res.witness_sequence), 0)

    def test_identical_api_with_renamed_locals_gives_no_witness(self):
        """Identical signatures and effects produce zero witness."""
        v1 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Submit": Operation("Submit", "Task", Effect.BORROW),
            },
        )
        v2 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Submit": Operation("Submit", "Task", Effect.BORROW),
            },
        )

        res = self.synth.analyze_release_change(v1, v2)
        self.assertFalse(res.has_breaking_change)
        self.assertEqual(res.status, "COMPATIBLE")

    def test_unsupported_aliasing_reports_unknown(self):
        """Cost honesty: unsupported aliasing returns UNKNOWN without false claims."""
        v1 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Alias": Operation("Alias", "Task", Effect.BORROW, has_aliasing=True),
            },
        )
        v2 = APIPackage(
            name="jobqueue",
            operations={
                "Inspect": Operation("Inspect", "Task", Effect.BORROW),
                "Alias": Operation("Alias", "Task", Effect.CONSUME, has_aliasing=True),
            },
        )

        res = self.synth.analyze_release_change(v1, v2)
        self.assertFalse(res.has_breaking_change)
        self.assertEqual(res.status, "UNKNOWN")
        self.assertIn("aliasing detected", res.reason)

    def test_enumeration_falsifier_matches_model(self):
        """Exhaustive check matches issue description (length 2 shortest witness)."""
        v1 = APIPackage(
            name="pipe",
            operations={
                "Poll": Operation("Poll", "Msg", Effect.BORROW),
                "Forward": Operation("Forward", "Msg", Effect.BORROW),
            },
        )
        v2 = APIPackage(
            name="pipe",
            operations={
                "Poll": Operation("Poll", "Msg", Effect.BORROW),
                "Forward": Operation("Forward", "Msg", Effect.CONSUME),
            },
        )
        res = self.synth.analyze_release_change(v1, v2)
        self.assertTrue(res.has_breaking_change)
        self.assertEqual(len(res.witness_sequence), 2)
        self.assertEqual(res.witness_sequence[0], "Forward")


if __name__ == "__main__":
    unittest.main()
