"""Unit tests for Spike W-3060 (wildcard #3060) — exact-value arithmetic.

    python3 -m unittest discover -s spikes/w3060-exact-value-arithmetic/tests -p "test_*.py" -v

Everything here is offline: the semantics table, the case emission, the
verdict parsing, and the results writer.  The one thing that needs a real
toolchain — the `hyb check` verdicts in RESULTS.md — is exercised by running
the script with `--hyb`, not here.
"""
import os
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import exact_value_arithmetic as eva


class TestWrapping(unittest.TestCase):
    def test_signed_wraps_two_complement(self):
        self.assertEqual(eva.wrap(200, "int8"), -56)
        self.assertEqual(eva.wrap(32768, "int16"), -32768)
        self.assertEqual(eva.wrap(-129, "int8"), 127)
        self.assertEqual(eva.wrap(2 ** 63, "int64"), -(2 ** 63))

    def test_unsigned_wraps_modulo_width(self):
        self.assertEqual(eva.wrap(300, "uint8"), 44)
        self.assertEqual(eva.wrap(2 ** 64, "uint64"), 0)


class TestPreRegisteredTable(unittest.TestCase):
    def test_every_prediction_holds(self):
        for row in eva.rows():
            with self.subTest(shape=row["id"]):
                self.assertTrue(row["prediction_held"], row)

    def test_window_is_not_empty(self):
        diverging = [r["id"] for r in eva.rows() if r["diverges"]]
        self.assertGreaterEqual(len(diverging), eva.H1_THRESHOLD, diverging)

    def test_the_headline_row(self):
        s1 = next(r for r in eva.rows() if r["id"] == "s1")
        self.assertEqual((s1["wrapped"], s1["exact"], s1["diverges"]), (-56, 200, True))


class TestCaseEmission(unittest.TestCase):
    def test_two_spellings_per_shape(self):
        with tempfile.TemporaryDirectory() as tmp:
            written = eva.emit_cases(tmp)
            self.assertEqual(len(written), 2 * len(eva.SHAPES))
            vars_src = pathlib.Path(tmp, "s1-vars.hy").read_text(encoding="utf-8")
            self.assertIn("var a int8 = 100", vars_src)
            self.assertIn("c := a + b", vars_src)
            consts_src = pathlib.Path(tmp, "s1-consts.hy").read_text(encoding="utf-8")
            self.assertIn("const a int8 = 100", consts_src)
            self.assertIn("_ = a + b", consts_src)


class TestVerdictParsing(unittest.TestCase):
    """Pin the parser on the two real outputs `hyb check` emits."""

    def test_parses_accept(self):
        self.assertEqual(
            eva.parse_verdict("cases/s1-vars.hy: OK (0 diagnostics)\n", 0),
            ("accept", "OK"),
        )

    def test_parses_first_error_code(self):
        real = ("error[HYB-E6002]: constant expression `a + b` overflows declared type `int8`"
                " (200 is outside [-128, 127]) (HYB-E6002)\n  --> cases/s1-consts.hy:4:9\n")
        self.assertEqual(eva.parse_verdict(real, 1), ("reject", "HYB-E6002"))

    def test_unknown_failure_is_not_silently_ok(self):
        self.assertEqual(eva.parse_verdict("boom", 1), ("reject", "?"))


class TestResultsWriter(unittest.TestCase):
    def test_without_hyb_marks_verdicts_unmeasured(self):
        with tempfile.TemporaryDirectory() as tmp:
            old_cases, old_results = eva.CASES, eva.RESULTS
            eva.CASES = os.path.join(tmp, "cases")
            eva.RESULTS = os.path.join(tmp, "RESULTS.md")
            try:
                diverging, held, total = eva.write_results(None)
                self.assertEqual(total, len(eva.SHAPES))
                self.assertEqual(held, len(eva.SHAPES))
                self.assertEqual(diverging, len(eva.SHAPES))
                body = pathlib.Path(eva.RESULTS).read_text(encoding="utf-8")
                self.assertIn("Not measured in this run", body)
                self.assertIn("**HELD**", body)
            finally:
                eva.CASES, eva.RESULTS = old_cases, old_results


if __name__ == "__main__":
    unittest.main()
