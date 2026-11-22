"""Offline regression tests for closure spacing in the escape-site scanner."""
import json
from pathlib import Path
import unittest

from escape_sites import find_escape_sites


class ClosureSpacingTests(unittest.TestCase):
    def test_spacing_and_comments_do_not_hide_capture_writes(self):
        for gap in ("", " ", "\t", "\r\n", "/* note */", "/* note\n */", "// note\n"):
            with self.subTest(gap=gap):
                source = ("fn main() {\n  hits := 0\n  http.handler(fn" + gap
                          + "() { hits++ })\n}\n")
                self.assertEqual(find_escape_sites(source), [{
                    "capture": "hits", "closure_line": 3,
                    "foreign_call": "http.handler",
                }])

    def test_spaced_nested_closure_is_scanned(self):
        source = "fn main() { http.handler(fn () { local(fn\t() { hits++ }) }) }"
        triples = find_escape_sites(source)
        # Preserve the documented M1 behavior: nested writes also appear in
        # the enclosing closure's capture set. This patch only changes spacing.
        self.assertEqual(len(triples), 2)
        self.assertTrue(all(t["capture"] == "hits" for t in triples))
        self.assertTrue(all(t["foreign_call"] == "http.handler" for t in triples))

    def test_identifiers_and_named_functions_are_not_anonymous_closures(self):
        for spelling in ("myfn ()", "fn_name ()", "fn handler()"):
            with self.subTest(spelling=spelling):
                self.assertEqual(find_escape_sites(
                    "http.handler(" + spelling + " { hits++ })"), [])

    def test_comment_and_string_contents_are_not_closures(self):
        source = 'http.handler("fn () { hits++ }") // fn () { hits++ }\n'
        self.assertEqual(find_escape_sites(source), [])

    def test_registered_m1_examples_keep_their_expected_counts(self):
        root = Path(__file__).resolve().parent.parent
        cases = json.loads((root / "tools/tests/escape_sites_cases.json").read_text())
        for case in cases["cases"]:
            with self.subTest(case=case["name"]):
                source = (root / case["file"]).read_text()
                self.assertEqual(len(find_escape_sites(source)), case["expected_count"])


if __name__ == "__main__":
    unittest.main()
