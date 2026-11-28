import json
import pathlib
import subprocess
import sys
import unittest

SPIKE_DIR = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SPIKE_DIR))

import effect_trampoline_verifier as v


class TestW4865EffectTrampolinesSpike(unittest.TestCase):
    def test_check_all_gates_pass(self):
        res = v.check_all()
        self.assertTrue(res["verified"], msg=json.dumps(res, indent=2))
        self.assertTrue(res["p1_static_admission"]["pass"])
        self.assertTrue(res["p2_state_machine"]["pass"])
        self.assertTrue(res["p3_cost_model"]["pass"])
        self.assertTrue(res["falsifiers"]["pass"])

    def test_p1_positive_fixtures_have_zero_diagnostics(self):
        for name in ("01-streaming-effect-session.hy", "02-structured-result-error.hy"):
            src = (SPIKE_DIR / "hy" / name).read_text(encoding="utf-8")
            self.assertEqual(v.check_effect_boundary_admission(src), [], msg=name)

    def test_p1_negative_fixture_triggers_all_five_codes(self):
        src = (SPIKE_DIR / "hy" / "03-rejected-effect-violations.hy").read_text(encoding="utf-8")
        codes = {d["code"] for d in v.check_effect_boundary_admission(src)}
        self.assertEqual(
            codes,
            {
                v.CODE_UNHANDLED_EFFECT,
                v.CODE_REENTRANT_HANDLER,
                v.CODE_SCRATCH_ESCAPE,
                v.CODE_LINEAR_SESSION,
                v.CODE_NON_POD_PAYLOAD,
            },
        )

    def test_p2_streaming_session_single_pin_and_zero_reentrancy(self):
        frame = v.RustBoundaryFrame(gen=101, total_chunks=16, emit_metrics=True)
        res = v.run_go_trampoline(frame, lambda i, _f: bytes([i]))
        self.assertTrue(res["ok"])
        self.assertEqual(res["value"], sum(range(1, 17)))
        self.assertEqual(res["pins_acquired"], 1)
        self.assertEqual(res["max_rust_depth"], 1)
        self.assertEqual(len(res["metrics_logged"]), 16)
        self.assertEqual(frame.owned_resources, 0)
        self.assertEqual(frame.state, v.FrameState.DONE)

    def test_p2_structured_result_error_propagates_without_leak(self):
        frame = v.RustBoundaryFrame(gen=102, total_chunks=5, fail_on_chunk=2)
        res = v.run_go_trampoline(frame, lambda i, _f: bytes([7, i]))
        self.assertFalse(res["ok"])
        self.assertEqual(res["error"], {"code": 422, "offset": 8})
        self.assertEqual(frame.owned_resources, 0)
        self.assertEqual(frame.state, v.FrameState.ERR_TERMINATED)

    def test_f1_mid_stream_panic_leaks_without_defer_abort_and_cleans_with_it(self):
        def boom(i, _f):
            if i == 2:
                raise RuntimeError("simulated Go panic")
            return b"\x05"

        leaky = v.RustBoundaryFrame(gen=201, total_chunks=4)
        with self.assertRaises(RuntimeError):
            v.run_go_trampoline(leaky, boom, enforce_defer_abort=False)
        self.assertEqual(leaky.owned_resources, 1)
        self.assertEqual(leaky.state, v.FrameState.YIELDED)

        safe = v.RustBoundaryFrame(gen=202, total_chunks=4)
        with self.assertRaises(RuntimeError):
            v.run_go_trampoline(safe, boom, enforce_defer_abort=True)
        self.assertEqual(safe.owned_resources, 0)
        self.assertEqual(safe.state, v.FrameState.ABORTED)

    def test_f2_reentrant_resume_rejected(self):
        frame = v.RustBoundaryFrame(gen=203, total_chunks=2)
        frame.state = v.FrameState.RUNNING
        with self.assertRaisesRegex(RuntimeError, "HYB-RT-REENTRANT"):
            frame.resume(203, bytearray(8), 0)

    def test_f3_stale_generation_rejected(self):
        frame = v.RustBoundaryFrame(gen=204, total_chunks=2)
        with self.assertRaisesRegex(RuntimeError, "HYB-RT-GEN"):
            frame.resume(203, bytearray(8), 0)

    def test_f4_double_resume_after_terminal_rejected(self):
        frame = v.RustBoundaryFrame(gen=205, total_chunks=1)
        res = v.run_go_trampoline(frame, lambda _i, _f: b"\x01")
        self.assertTrue(res["ok"])
        with self.assertRaisesRegex(RuntimeError, "HYB-RT-CONSUMED"):
            frame.resume(205, bytearray(8), 0)

    def test_f5_scratch_overflow_rejected_and_aborted(self):
        frame = v.RustBoundaryFrame(gen=206, total_chunks=2, scratch_cap=8)
        with self.assertRaisesRegex(RuntimeError, "HYB-RT-BOUNDS"):
            v.run_go_trampoline(frame, lambda _i, _f: b"0123456789", enforce_defer_abort=True)
        self.assertEqual(frame.owned_resources, 0)
        self.assertEqual(frame.state, v.FrameState.ABORTED)

    def test_p3_cost_model_crossover_and_sensitivity(self):
        base = {r["n"]: r for r in v.compute_cost_model(yield_ns=2.5)}
        self.assertEqual(base[2]["speedup"], 1.77)
        self.assertEqual(base[8]["speedup"], 4.87)
        self.assertEqual(base[32]["speedup"], 8.65)
        self.assertEqual(base[1000]["speedup"], 11.54)

        stress = {r["n"]: r for r in v.compute_cost_model(yield_ns=20.0)}
        self.assertGreater(stress[2]["speedup"], 1.1)
        self.assertGreater(stress[1000]["speedup"], 2.5)

    def test_cli_check_json_exits_zero(self):
        proc = subprocess.run(
            [sys.executable, str(SPIKE_DIR / "effect_trampoline_verifier.py"), "--check", "--json"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 0, msg=proc.stderr)
        payload = json.loads(proc.stdout)
        self.assertTrue(payload["verified"])


if __name__ == "__main__":
    unittest.main()
