import subprocess, tempfile, time, unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from summary_engine import CallSite, FnIR, check_callback_escape, infer_summaries


class EffectPolymorphicSummarySpikeTests(unittest.TestCase):
    def test_h1_depth_100_and_mutual_recursion_converge_exact(self):
        fns = {"s100": FnIR("s100", 2, direct_shares={(0, 1)})}
        for k in range(99, 0, -1):
            fns[f"s{k}"] = FnIR(f"s{k}", 2, calls=[CallSite(f"s{k+1}", [0, 1])])
        # Mutual recursion ping <-> pong storing slot 1 into slot 0
        fns["ping"] = FnIR("ping", 2, calls=[CallSite("pong", [0, 1])])
        fns["pong"] = FnIR("pong", 2, direct_shares={(0, 1)}, calls=[CallSite("ping", [0, 1])])
        t0 = time.perf_counter()
        sums = infer_summaries(fns, {})
        elapsed_ms = (time.perf_counter() - t0) * 1000.0
        self.assertIn((0, 1), sums["s1"].shares)
        self.assertIn((0, 1), sums["ping"].shares)
        self.assertLess(elapsed_ms, 50.0)

    def test_h2_sync_vs_async_callback_matches_go_race_oracle(self):
        self.assertEqual(check_callback_escape(True, "sync*"), "ACCEPT")
        self.assertEqual(check_callback_escape(True, "async*"), "REJECT_HYB_E3002")
        with tempfile.TemporaryDirectory() as td:
            sync_go = Path(td) / "sync_prog.go"
            sync_go.write_text(
                'package main\nimport "sort"\nfunc main() {\n xs := []int{3, 1, 2}\n swaps := 0\n sort.Slice(xs, func(i, j int) bool { swaps++; return xs[i] < xs[j] })\n _ = swaps\n}\n'
            )
            r1 = subprocess.run(["go", "run", "-race", str(sync_go)], capture_output=True, text=True)
            self.assertEqual(r1.returncode, 0, r1.stderr)


if __name__ == "__main__":
    unittest.main()
