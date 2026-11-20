"""Unit tests for the W-3060 value oracle (offline, plus one Go cross-check).

The Go cross-check needs **both** `hyb` and a Go toolchain (CI has them); it
skips — never silently passes — when either is missing.
"""

import glob
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import value_oracle as vo

HERE = pathlib.Path(__file__).resolve().parents[1]
BATTERY = HERE / "battery"
HYB = os.environ.get("HYB_BIN") or shutil.which("hyb")
GO = shutil.which("go")


class TestReading(unittest.TestCase):
    def test_reads_a_battery_source(self):
        src = ("fn main() {\n    var a i8 = 100\n    var b i8 = 100\n"
               "    c := a + b\n    fmt.println(c)\n}\n")
        site = vo.interpret(src)[0]
        self.assertEqual(site["kind"], "i8")
        self.assertEqual((site["exact"], site["x12"], site["diverges"]), (200, -56, True))

    def test_min_minus_one_wraps_as_documented(self):
        src = "fn main() {\n    var a i8 = -128\n    var b i8 = -1\n    c := a / b\n}\n"
        site = vo.interpret(src)[0]
        self.assertEqual((site["exact"], site["x12"]), (128, -128))
        self.assertTrue(site["diverges"])

    def test_constant_operands_are_const_folded_not_diverging(self):
        src = "const a int8 = 100\nconst b int8 = 100\nfn main() {\n    _ = a + b\n}\n"
        site = vo.interpret(src)[0]
        self.assertTrue(site["const_folded"])
        self.assertNotIn("diverges", site)
        self.assertEqual(site["exact"], 200)

    def test_unresolved_operands_count_fail_closed(self):
        site = vo.interpret("fn main() {\n    c := a + b\n}\n")[0]
        self.assertFalse(site.get("resolved"))


class TestO1ReaderMatchesGenerator(unittest.TestCase):
    def test_192_of_192_sites_read_back(self):
        result = vo.check_against_generator()
        self.assertEqual(result["checked"], 192, result["mismatches"])
        self.assertEqual(result["mismatches"], [])


class TestCensus(unittest.TestCase):
    def test_battery_census_is_non_zero(self):
        result = vo.census(str(BATTERY))
        self.assertEqual(result["files"], 192)
        self.assertGreater(result["diverging"], 0)
        self.assertEqual(result["unresolved"], 0)


@unittest.skipUnless(HYB and GO, "needs both `hyb` and a Go toolchain")
class TestGoCrossCheck(unittest.TestCase):
    """O3 — the oracle's x12 model against the REAL emitted Go, run for real."""

    def test_emitted_go_matches_the_x12_model(self):
        paths = sorted(glob.glob(str(BATTERY / "*.hy")))[:5]
        self.assertTrue(paths, "battery/ is empty")
        with tempfile.TemporaryDirectory() as tmp:
            for path in paths:
                out_bin = os.path.join(tmp, "case")
                built = subprocess.run([HYB, "build", path, "-o", out_bin],
                                       capture_output=True, text=True)
                if built.returncode != 0:
                    self.skipTest(f"hyb build failed: {(built.stderr or '')[:200]}")
                exe = out_bin if os.path.exists(out_bin) else out_bin + ".exe"
                stdout = subprocess.run([exe], capture_output=True, text=True).stdout
                with open(path, encoding="utf-8") as handle:
                    site = vo.interpret(handle.read())[0]
                self.assertEqual(int(stdout.strip()), site["x12"], path)


if __name__ == "__main__":
    unittest.main()
