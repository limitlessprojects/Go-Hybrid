"""Unit tests for the W-3060 narrow-width battery generator (offline)."""

import os
import pathlib
import re
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import battery_generator as bg


class TestGrid(unittest.TestCase):
    def test_grid_is_the_locked_8x8x3_and_is_deterministic(self):
        first, second = bg.generate(3060), bg.generate(3060)
        self.assertEqual(len(first), 8 * 8 * 3)
        self.assertEqual([c["id"] for c in first], [c["id"] for c in second])
        self.assertEqual([(c["a"], c["b"]) for c in first],
                         [(c["a"], c["b"]) for c in second])

    def test_slugs_are_filesystem_safe(self):
        bad = [c["slug"] for c in bg.generate(3060) if not re.fullmatch(r"[a-z0-9-]+", c["slug"])]
        self.assertEqual(bad, [])


class TestBoundaries(unittest.TestCase):
    def test_battery_is_not_vacuous(self):
        stat = bg.metrics(bg.generate(3060))
        self.assertGreaterEqual(stat["diverging_share"], 1 / 3, stat)

    def test_min_div_minus_one_is_the_documented_wrap_case(self):
        cases = bg.generate(3060)
        case = next(c for c in cases
                    if c["op"] == "/" and c["draw"] == "uniform" and c["kind"] == "i8")
        lo, _hi = bg.range_of("i8")
        self.assertEqual((case["a"], case["b"]), (lo, -1))
        self.assertEqual(case["wrapped"], lo)        # Go: q == x for min / -1
        self.assertNotEqual(case["wrapped"], case["exact"])
        self.assertTrue(case["diverges"])

    def test_non_overflowing_operators_discharge_fully(self):
        stat = bg.metrics(bg.generate(3060))
        for op in ("%", ">>", "|"):
            self.assertEqual(stat["per_op"][op]["exact"], stat["per_op"][op]["n"], op)


class TestEmission(unittest.TestCase):
    def test_emit_writes_one_program_per_site(self):
        with tempfile.TemporaryDirectory() as tmp:
            cases = bg.generate(3060)
            written = bg.emit(cases, tmp)
            self.assertEqual(len(written), len(cases))
            text = pathlib.Path(tmp, "i8-add-maxsaturated.hy").read_text(encoding="utf-8")
            self.assertIn("var a i8 = 127", text)
            self.assertIn("c := a + b", text)


if __name__ == "__main__":
    unittest.main()
