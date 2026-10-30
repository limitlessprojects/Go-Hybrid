"""Unit and integration tests for the W-2220 M1 demotion gate.

Run: python3 -m unittest discover -s spikes/w2220-guarantees-as-code -p 'test_*.py' -v

Integration tests need a built checker (`cargo build -p hyb`) and `go` on PATH;
they skip cleanly when either is missing.
"""

from __future__ import annotations

import json
import shutil
import sys
import unittest
from pathlib import Path

SPIKE_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SPIKE_DIR))

import guarantees_gate as gate  # noqa: E402

ROOT = gate.REPO_ROOT
HYB = ROOT / "compiler" / "target" / "debug" / "hyb"


def claims_rows() -> dict[int, dict]:
    return gate.claim_rows(json.loads((ROOT / "guarantees.json").read_text()))


def policy() -> dict:
    return json.loads((SPIKE_DIR / "demotion_policy.json").read_text())


def row(strength: str = "no-claim") -> dict:
    return {"id": 99, "claim": "**No claim** — example", "strength": strength}


class DemotionStateTests(unittest.TestCase):
    def test_unregistered_row_is_not_a_claim(self):
        state = gate.row_state(99, row(), None)
        self.assertEqual(state["status"], "unregistered")
        self.assertIn("unregistered", state["effective_claim"])

    def test_prose_only_needs_owner_expiry_and_debt(self):
        pol = {"status": "prose-only", "verifier": {"kind": "argument"}}
        state = gate.row_state(99, row(), pol)
        self.assertEqual(state["status"], "invalid")
        self.assertTrue(any("owner" in p for p in state["problems"]))
        self.assertTrue(any("debt" in p for p in state["problems"]))
        self.assertTrue(any("expiry" in p for p in state["problems"]))

    def test_guaranteed_row_cannot_be_prose_only(self):
        pol = {
            "status": "prose-only",
            "verifier": {"kind": "argument"},
            "owner": "@x",
            "expires": "2099-01-01",
            "debt": "D-1",
        }
        state = gate.row_state(99, row("guaranteed"), pol)
        self.assertEqual(state["status"], "invalid")
        self.assertTrue(any("guaranteed" in p for p in state["problems"]))

    def test_expired_prose_only_demotes_to_no_claim(self):
        pol = {
            "status": "prose-only",
            "verifier": {"kind": "argument"},
            "owner": "@x",
            "expires": "2020-01-01",
            "debt": "D-1",
        }
        state = gate.row_state(99, row(), pol)
        self.assertEqual(state["status"], "demoted")
        self.assertIn(gate.DEMOTED_PREFIX, state["effective_claim"])

    def test_argument_cannot_be_executable(self):
        pol = {"status": "executable", "verifier": {"kind": "argument"}}
        state = gate.row_state(99, row(), pol)
        self.assertEqual(state["status"], "invalid")

    def test_empty_registry_is_an_error(self):
        with self.assertRaises(ValueError):
            gate.claim_rows({"rows": []})


class RenderTests(unittest.TestCase):
    def test_render_shows_every_state(self):
        rows = {1: {"id": 1, "claim": "guaranteed text", "strength": "guaranteed"},
                9: {"id": 9, "claim": "no claim text", "strength": "no-claim"}}
        policy = {
            "rows": {
                "1": {"status": "executable",
                      "verifier": {"kind": "corpus_pair"},
                      "owner": "@x", "review_by": "2099-01-01"},
                "9": {"status": "prose-only",
                      "verifier": {"kind": "argument"},
                      "owner": "@x", "expires": "2020-01-01", "debt": "D-9"},
            }
        }
        states = {rid: gate.row_state(rid, r, policy["rows"].get(str(rid))) for rid, r in rows.items()}
        text = gate.render(rows, states)
        self.assertIn("guaranteed text", text)
        self.assertIn("DEMOTED", text.upper())
        self.assertIn("D-9", text)


@unittest.skipUnless(HYB.exists(), "checker not built (cargo build -p hyb)")
class IntegrationTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("go"), "go not on PATH")
    def test_gate_passes_and_self_test_catches_mutations(self):
        rows = claims_rows()
        self.assertEqual(len(rows), 20)
        pol = policy()
        states, problems = gate.check_structure(rows, pol)
        self.assertEqual(problems, [], problems)
        self.assertEqual(states[1]["status"], "executable")
        self.assertEqual(states[8]["status"], "executable")
        self.assertEqual(states[16]["status"], "executable")
        self.assertEqual(gate.self_test(rows, pol, str(HYB), "go", ROOT), 0)


if __name__ == "__main__":
    unittest.main()
