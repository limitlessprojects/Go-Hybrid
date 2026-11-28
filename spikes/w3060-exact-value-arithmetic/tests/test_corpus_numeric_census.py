"""Unit tests for the W-3060 corpus numeric census (offline, synthetic corpus)."""

import os
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import corpus_numeric_census as cnc


def _write(directory, name, body):
    with open(os.path.join(directory, name), "w", encoding="utf-8", newline="\n") as handle:
        handle.write(body)


class TestCensus(unittest.TestCase):
    def test_counts_declarations_narrow_files_and_both_declared_sites(self):
        with tempfile.TemporaryDirectory() as tmp:
            _write(tmp, "a.hy", "fn main() {\n    var x u8 = 1\n    var y u8 = 2\n"
                              "    z := x + y\n    w := 1 + 2\n}\n")
            _write(tmp, "b.hy", "fn main() {\n    var big int64 = 1\n    var small int8 = 2\n"
                              "    _ = big + small\n}\n")
            _write(tmp, "c.hy", "fn main() {\n    n := 1 + 2\n}\n")
            report = cnc.census(tmp)

        self.assertEqual(report["files"], 3)
        self.assertEqual(report["declarations"], 4)
        self.assertEqual(report["narrow_declarations"], 3)          # u8, u8, int8
        self.assertEqual(report["files_with_any_declaration"], 2)   # c.hy declares nothing
        self.assertEqual(report["sites_both_declared"], 2)          # x+y and big+small
        self.assertEqual(report["sites_both_declared_narrow"], 1)   # x+y
        self.assertEqual(report["sites_width_mismatch"], 1)         # big+small
        self.assertEqual(report["files_with_operators"], 3)

    def test_rendered_report_mentions_the_denominator(self):
        with tempfile.TemporaryDirectory() as tmp:
            _write(tmp, "a.hy", "fn main() {\n    var x u8 = 1\n    var y u8 = 2\n    _ = x + y\n}\n")
            text = cnc.render(cnc.census(tmp))
        self.assertIn("sites where BOTH operand names have a declared numeric type", text)
        self.assertIn("total: 1", text)

    def test_missing_directory_is_an_error_not_a_silent_zero(self):
        with self.assertRaises(FileNotFoundError):
            cnc.census(os.path.join(tempfile.gettempdir(), "w3060-does-not-exist"))


if __name__ == "__main__":
    unittest.main()
