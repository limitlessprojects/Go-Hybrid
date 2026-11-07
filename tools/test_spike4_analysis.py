"""Offline regressions for Spike 4's S4 latency-overhead report."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


ANALYZER = (Path(__file__).resolve().parents[1]
            / "spikes/spike4-scheduling/work/analyze.py")
SPEC = importlib.util.spec_from_file_location("spike4_analyze", ANALYZER)
analyze = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(analyze)


def cell(delay_ms, p99_us, **overrides):
    row = {"mech": "M3", "n": 1000, "frac_pct": 1, "d_ms": delay_ms,
           "p99_us": p99_us, "done": 1000, "capped": False, "wall_s": 1}
    row.update(overrides)
    return row


class LatencyOverheadTests(unittest.TestCase):
    def report(self, rows, filename="go/matrix.sandbox.jsonl"):
        # Redirect only the input directory; run the real loader and main().
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / filename
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("".join(json.dumps(row) + "\n" for row in rows),
                            encoding="utf-8")
            output = io.StringIO()
            with patch.object(analyze, "WORK", root), contextlib.redirect_stdout(output):
                analyze.main()
        return [line.strip() for line in output.getvalue().splitlines()
                if "p99 latency overhead:" in line]

    def assert_overhead(self, delay_ms, p99_us, expected, verdict):
        self.assertEqual(self.report([cell(delay_ms, p99_us)]), [
            f"(1k,1%,{delay_ms}ms) p99 latency overhead: "
            f"{expected} ms (<=5ms) -> {verdict}"])

    def test_four_ms_overhead_passes(self):
        self.assert_overhead(100, 104000, "4.00", "PASS")

    def test_five_ms_boundary_passes(self):
        for delay_ms, p99_us in ((1, 6000), (100, 105000)):
            with self.subTest(delay_ms=delay_ms):
                self.assert_overhead(delay_ms, p99_us, "5.00", "PASS")

    def test_just_above_boundary_fails_even_when_display_rounds_to_five(self):
        for delay_ms, p99_us in ((1, 6001), (100, 105001)):
            with self.subTest(delay_ms=delay_ms):
                self.assert_overhead(delay_ms, p99_us, "5.00", "FAIL")

    def test_zero_overhead(self):
        for delay_ms, p99_us in ((1, 1000), (100, 100000)):
            with self.subTest(delay_ms=delay_ms):
                self.assert_overhead(delay_ms, p99_us, "0.00", "PASS")

    def test_published_sandbox_sample_remains_fail(self):
        # Published go/matrix.sandbox.jsonl: 157.378482ms - 100ms.
        self.assert_overhead(100, 157378.482, "57.38", "FAIL")

    def test_capped_cells_are_omitted(self):
        self.assertEqual(self.report([
            cell(1, 6000, capped=True), cell(100, 105000, capped=True)]), [])

    def test_missing_cells_are_omitted(self):
        self.assertEqual(self.report([]), [])

    def test_other_mechanisms_and_concurrency_are_omitted(self):
        self.assertEqual(self.report([
            cell(1, 6000, mech="M0"), cell(100, 105000, n=10000)]), [])

    def test_fallback_matrix_path(self):
        self.assertEqual(self.report([cell(100, 104000)], "matrix.sandbox.jsonl"), [
            "(1k,1%,100ms) p99 latency overhead: 4.00 ms (<=5ms) -> PASS"])


if __name__ == "__main__":
    unittest.main()
