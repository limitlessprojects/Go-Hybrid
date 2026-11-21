import sys
import unittest
from pathlib import Path
from unittest.mock import patch

SPIKE_DIR = Path(__file__).resolve().parents[1]
WORK = SPIKE_DIR / "work"
FIXTURE = SPIKE_DIR / "fixtures" / "go1.23.4_stext_excerpts.txt"
sys.path.insert(0, str(WORK))
import escape_oracle as eo  # noqa: E402


class TestEscapeOracle(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.res = eo.verify_package(WORK, eo.TARGETS)
        cls.leaf_audit = eo.audit_linked_leaf_primitives(WORK)

    def test_19_of_19_verdicts_and_m_blind_spots(self) -> None:
        verdicts = self.res["verdicts"]
        self.assertEqual(len(verdicts), 19)
        for name, v in verdicts.items():
            self.assertEqual(v["certified_noheap"], name.startswith("K"), msg=f"{name}: {v}")
        for blind in (
            "F6SliceAppend",
            "F7TransitiveCallee",
            "F8IndirectInterface",
            "F9OpenDeferPlusFuncVal",
            "F10OpenDeferAllocFuncN",
            "F11RecoverBoundsTrap",
            "F12InlinedHelperHyLine",
        ):
            self.assertTrue(verdicts[blind]["m_only_noheap"], msg=blind)
            self.assertFalse(verdicts[blind]["certified_noheap"], msg=blind)

    def test_probe1_leaf_primitives_have_zero_outgoing_calls(self) -> None:
        self.assertEqual(set(self.leaf_audit.keys()), set(eo.LEAF_MEM_RT))
        for sym, calls in self.leaf_audit.items():
            self.assertEqual(calls, [], msg=f"{sym} unexpectedly calls {calls}")

    def test_probe1_recover_unmasks_cold_bounds_trap_and_unknown_rt_fails_closed(self) -> None:
        f11 = self.res["verdicts"]["F11RecoverBoundsTrap"]["asm_witnesses"]
        self.assertTrue(any("runtime.gorecover" in w for w in f11), msg=f11)
        self.assertTrue(any("runtime.panicIndex" in w for w in f11), msg=f11)

        synth_asm = "\n".join([
            "hybrid.spike/noheap.FutureRt STEXT size=32 args=0x0 locals=0x8 funcid=0x0 align=0x0",
            "\t0x0000 00000 (future.hy:10[work/future.go:10])\tTEXT\thybrid.spike/noheap.FutureRt(SB), ABIInternal, $8-0",
            "\t0x0005 00005 (future.hy:11[work/future.go:11])\tCALL\truntime.unknownGo124Helper(SB)",
            "\t0x000a 00010 (future.hy:12[work/future.go:12])\tCALL\truntime.gopanic(SB)",
        ])
        syms = eo.parse_stext_stream(synth_asm)
        bad = eo.certify_symbol("hybrid.spike/noheap.FutureRt", syms)
        self.assertEqual(
            bad,
            [
                "future.hy:11: CALL runtime.unknownGo124Helper [HYB-E7001]",
                "future.hy:12: CALL runtime.gopanic [HYB-E7001]",
            ],
        )

    def test_probe1_unpinned_toolchain_fails_closed(self) -> None:
        with patch.object(eo, "detect_go_version", return_value="go1.24.0"):
            with self.assertRaisesRegex(RuntimeError, "HYB-E7003"):
                eo.verify_package(WORK, ["K1FixedScratch"], enforce_toolchain_pin=True)

    def test_probe2_per_slot_opendefer_vs_arbitrary_funcval_rcallind(self) -> None:
        self.assertEqual(self.res["verdicts"]["K4OpenCodedDefer"]["asm_witnesses"], [])
        f9 = self.res["verdicts"]["F9OpenDeferPlusFuncVal"]["asm_witnesses"]
        self.assertEqual(
            f9,
            ["kernels.hy:161: CALL <indirect:CX> [HYB-E7002 uncertified R_CALLIND]"],
        )
        f10 = self.res["verdicts"]["F10OpenDeferAllocFuncN"]["asm_witnesses"]
        self.assertEqual(
            f10,
            ["kernels.hy:171 -> kernels.hy:168: CALL runtime.makeslice [HYB-E7001]"],
        )

    def test_probe3_hy_line_attribution_across_inlined_bodies(self) -> None:
        self.assertEqual(self.res["verdicts"]["K7InlinedCleanHelper"]["asm_witnesses"], [])
        f12 = self.res["verdicts"]["F12InlinedHelperHyLine"]["asm_witnesses"]
        self.assertEqual(
            f12,
            ["kernels.hy:187 -> kernels.hy:28 (inlined): CALL runtime.growslice [HYB-E7001]"],
        )

    def test_committed_go1234_stext_fixture_replays_offline(self) -> None:
        self.assertTrue(FIXTURE.is_file())
        syms = eo.parse_stext_stream(FIXTURE.read_text())
        self.assertEqual(eo.certify_symbol(eo.PKG + "K4OpenCodedDefer", syms), [])
        self.assertEqual(eo.certify_symbol(eo.PKG + "K7InlinedCleanHelper", syms), [])
        self.assertEqual(
            eo.certify_symbol(eo.PKG + "F9OpenDeferPlusFuncVal", syms),
            ["kernels.hy:161: CALL <indirect:CX> [HYB-E7002 uncertified R_CALLIND]"],
        )
        self.assertEqual(
            eo.certify_symbol(eo.PKG + "F12InlinedHelperHyLine", syms),
            ["kernels.hy:187 -> kernels.hy:28 (inlined): CALL runtime.growslice [HYB-E7001]"],
        )


if __name__ == "__main__":
    unittest.main()
