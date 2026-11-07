#!/usr/bin/env python3
"""Offline unit tests for tools/check_guarantees.py: corpus boundary checks,
anchored/traversal markdown links, and generated-table marker validation."""

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
import check_guarantees as cg  # noqa: E402
import test_check_guarantees as base  # noqa: E402


class CheckGuaranteesBoundsTests(unittest.TestCase):
    def test_sibling_prefix_and_traversal_fixtures_rejected_as_outside_corpus(self):
        sibling_rel = "tests/checker-corpus-extra/01-sibling.hy"
        traversal_rel = "tests/checker-corpus/../other/01-escape.hy"
        tmp, root = base.build(
            rows=[
                base.row(
                    1,
                    fixtures=[
                        {"path": sibling_rel, "expect": "reject"},
                        {"path": traversal_rel, "expect": "reject"},
                    ],
                )
            ],
            fixtures={
                sibling_rel: base.fixture("REJECT A1c"),
                "tests/other/01-escape.hy": base.fixture("REJECT A1c"),
            },
        )
        self.addCleanup(tmp.cleanup)
        problems = "\n".join(base.problems_of(root))
        self.assertIn(f"fixture {sibling_rel} is outside tests/checker-corpus", problems)
        self.assertIn(f"fixture {traversal_rel} is outside tests/checker-corpus", problems)

    def test_anchored_evidence_link_resolves_and_traversal_link_fails(self):
        tmp, root = base.build(
            rows=[base.row(1, evidence="[spec](SPEC-LANGUAGE.md#93-spawn)")],
            fixtures={
                base.FIXTURE_1: base.fixture("REJECT A1c"),
                "SPEC-LANGUAGE.md": "# Spec\n\n## 9.3 Spawn\n",
            },
        )
        self.addCleanup(tmp.cleanup)
        self.assertEqual(base.problems_of(root), [])

        outside = root.parent / (root.name + "-outside.md")
        outside.write_text("outside", encoding="utf-8")
        self.addCleanup(lambda: outside.unlink(missing_ok=True))
        tmp2, root2 = base.build(
            rows=[base.row(1, evidence=f"[escape](../{outside.name})")]
        )
        self.addCleanup(tmp2.cleanup)
        self.assertIn("does not exist", "\n".join(base.problems_of(root2)))

    def test_inverted_or_duplicate_markers_and_eof_end_marker(self):
        tmp, root = base.build()
        self.addCleanup(tmp.cleanup)
        doc_path = root / "GUARANTEES.md"
        # Strip trailing newline after END and verify write_doc preserves tail cleanly
        doc_no_nl = f"# Title\n\n{cg.BEGIN}\nold\n{cg.END}TAIL"
        doc_path.write_text(doc_no_nl, encoding="utf-8")
        cg.write_doc(root, root / "guarantees.json")
        self.assertTrue(doc_path.read_text(encoding="utf-8").endswith(f"{cg.END}\nTAIL"))

        # Inverted markers fail check() and raise Misuse in write_doc()
        doc_path.write_text(f"# Title\n{cg.END}\n{cg.BEGIN}\n", encoding="utf-8")
        self.assertIn(
            "duplicate or inverted generated-table markers",
            "\n".join(base.problems_of(root)),
        )
        with self.assertRaises(cg.Misuse):
            cg.write_doc(root, root / "guarantees.json")


if __name__ == "__main__":
    unittest.main()
