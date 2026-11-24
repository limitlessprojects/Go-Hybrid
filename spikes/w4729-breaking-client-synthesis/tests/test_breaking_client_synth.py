#!/usr/bin/env python3
import unittest
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from breaking_client_synth_model import (
    Effect, State, apply_op, run_sequence, enumerate_all_sequences, find_shortest_witness
)

class TestBreakingClientSynth(unittest.TestCase):
    def setUp(self):
        self.old_contract = {"Submit": Effect.PRESERVES, "Inspect": Effect.PRESERVES}
        self.new_contract = {"Submit": Effect.CONSUMES, "Inspect": Effect.PRESERVES}

    def test_enumeration_count(self):
        seqs = enumerate_all_sequences(["Submit", "Inspect"], max_len=6)
        self.assertEqual(len(seqs), 127)

    def test_borrow_to_consume_census(self):
        seqs = enumerate_all_sequences(["Submit", "Inspect"], max_len=6)
        breaking = 0
        for s in seqs:
            o_ok, _, _ = run_sequence(s, self.old_contract)
            n_ok, _, _ = run_sequence(s, self.new_contract)
            if o_ok and not n_ok:
                breaking += 1
        self.assertEqual(breaking, 114)

    def test_shortest_witness(self):
        sw = find_shortest_witness(self.old_contract, self.new_contract, ops=["Inspect", "Submit"])
        self.assertIsNotNone(sw)
        path, idx, diag = sw
        self.assertEqual(path, ["Submit", "Inspect"])
        self.assertEqual(idx, 1)
        self.assertEqual(diag, "HYB-E3003")

    def test_relaxation_zero_breaking(self):
        seqs = enumerate_all_sequences(["Submit", "Inspect"], max_len=6)
        breaking = 0
        for s in seqs:
            o_ok, _, _ = run_sequence(s, self.new_contract)
            n_ok, _, _ = run_sequence(s, self.old_contract)
            if o_ok and not n_ok:
                breaking += 1
        self.assertEqual(breaking, 0)

    def test_unchanged_zero_breaking(self):
        seqs = enumerate_all_sequences(["Submit", "Inspect"], max_len=6)
        breaking = 0
        for s in seqs:
            o_ok, _, _ = run_sequence(s, self.old_contract)
            n_ok, _, _ = run_sequence(s, self.old_contract)
            if o_ok and not n_ok:
                breaking += 1
        self.assertEqual(breaking, 0)

    def test_unsupported_aliasing(self):
        c_unsupported = {"Submit": Effect.UNSUPPORTED}
        nxt, ok, diag = apply_op(c_unsupported["Submit"], State.LIVE)
        self.assertFalse(ok)
        self.assertEqual(diag, "UNKNOWN")

if __name__ == "__main__":
    unittest.main()
