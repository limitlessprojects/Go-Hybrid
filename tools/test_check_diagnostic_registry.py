"""Tests for tools/check_diagnostic_registry.py (issue #4460 class).

The guard exists because the direction registry → GUARANTEES is already policed
(`check_guarantees.py` rejects a row naming an unregistered code) while the
opposite direction — compiler source → registry — was not, so `HYB-E4004` and
`HYB-E4005` shipped printed and unexplainable.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import check_diagnostic_registry as guard


def registry(*codes: str) -> dict:
    return {
        "version": 1,
        "updated": "2026-11-14",
        "format": "test",
        "categories": {"E4": "nil / zero-value safety"},
        "codes": [
            {
                "code": code,
                "name": code.lower(),
                "category": "nil-safety",
                "severity": "error",
                "title": "t",
                "summary": "s",
            }
            for code in codes
        ],
    }


class RegistryGuardTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "compiler" / "crates" / "hy-check" / "src").mkdir(parents=True)
        registry_path = self.root / guard.REGISTRY_REL
        registry_path.parent.mkdir(parents=True, exist_ok=True)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def write_registry(self, *codes: str) -> None:
        (self.root / guard.REGISTRY_REL).write_text(json.dumps(registry(*codes)), encoding="utf-8")

    def write_source(self, body: str) -> None:
        source = self.root / "compiler" / "crates" / "hy-check" / "src" / "checker.rs"
        source.write_text(body, encoding="utf-8")

    def test_emitted_and_registered_passes(self) -> None:
        self.write_registry("HYB-E3003", "HYB-E4004")
        self.write_source('diag("HYB-E4004"); diag("HYB-E3003");\n')
        self.assertEqual(guard.collect_problems(self.root), [])

    def test_emitted_but_unregistered_is_reported_with_location(self) -> None:
        self.write_registry("HYB-E3003")
        self.write_source('fn a() {}\ndiag("HYB-E4004");\n')
        problems = guard.collect_problems(self.root)
        self.assertEqual(len(problems), 1)
        self.assertIn("checker.rs:2", problems[0])
        self.assertIn("HYB-E4004", problems[0])

    def test_comment_mention_counts(self) -> None:
        # A doc comment naming a code that does not exist is the same drift one
        # step earlier, so mentions are scanned, not only string literals.
        self.write_registry("HYB-E3003")
        self.write_source("// see HYB-E4005 for the lifecycle rule\nfn a() {}\n")
        problems = guard.collect_problems(self.root)
        self.assertEqual(len(problems), 1)
        self.assertIn("HYB-E4005", problems[0])

    def test_registered_but_unused_is_not_a_problem(self) -> None:
        self.write_registry("HYB-E3003", "HYB-E7002")
        self.write_source('diag("HYB-E3003");\n')
        self.assertEqual(guard.collect_problems(self.root), [])

    def test_other_crates_are_scanned_too(self) -> None:
        self.write_registry("HYB-E3003")
        emit_src = self.root / "compiler" / "crates" / "hy-emit" / "src"
        emit_src.mkdir(parents=True)
        (emit_src / "emit.rs").write_text('fail("HYB-E1002");\n', encoding="utf-8")
        problems = guard.collect_problems(self.root)
        self.assertEqual(len(problems), 1)
        self.assertIn("hy-emit", problems[0])

    def test_missing_registry_reports_and_does_not_pass(self) -> None:
        self.write_source('diag("HYB-E3003");\n')
        problems = guard.collect_problems(self.root)
        self.assertEqual(len(problems), 1)
        self.assertIn("no registry", problems[0])


if __name__ == "__main__":
    unittest.main()
