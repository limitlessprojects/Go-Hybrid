"""Verification tests for W-4843 (#4843): Monotonic Ownership-to-Value Sealing
(`seal move x` -> `sealed[T]`, Rule O5.7 / Sendable Class 4b).

Covers both halves of the spike:
  - checker/seal_checker.py: the static admission model (ownership,
    interior-sealability, storage-component exclusivity, corpus #19 twin,
    and the GUARANTEES.md Rows 5/9 foreign-callback flip).
  - work/: the Go runtime representation (SealedMap/SealedSlice, the P1/P3
    positive hypotheses, and the F1/F2/F3 declared falsifiers), driven via
    `go test`.
"""

from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path

SPIKE_DIR = Path(__file__).resolve().parents[1]
HY_DIR = SPIKE_DIR / "hy"
WORK_DIR = SPIKE_DIR / "work"
CHECKER_DIR = SPIKE_DIR / "checker"

sys.path.insert(0, str(CHECKER_DIR))

from seal_checker import (  # noqa: E402
    Binding,
    SealError,
    TypeDesc,
    foreign_callback_capture,
    is_sealable,
    move_then_read,
    seal_move,
    seal_move_twin,
)


def map_type() -> TypeDesc:
    return TypeDesc(kind="map", fields=(TypeDesc(kind="string"), TypeDesc(kind="int")))


class TestSpikeLayout(unittest.TestCase):
    def test_results_doc_and_fixtures_exist(self) -> None:
        results = (SPIKE_DIR / "RESULTS.md").read_text(encoding="utf-8")
        self.assertIn("4843", results)
        for name in (
            "01-seal-move-corpus19-twin.hy",
            "02-seal-move-reject-nonowned-and-interior-closure.hy",
            "03-row9-foreign-callback-flip.hy",
        ):
            self.assertTrue((HY_DIR / name).is_file(), f"missing {name}")
        for name in ("go.mod", "sealed.go", "sealed_test.go", "bench_test.go"):
            self.assertTrue((WORK_DIR / name).is_file(), f"missing work/{name}")

    def test_hy_fixtures_contain_expected_shapes(self) -> None:
        twin = (HY_DIR / "01-seal-move-corpus19-twin.hy").read_text(encoding="utf-8")
        self.assertIn("seal move agg", twin)
        self.assertIn("snap.len()", twin)

        reject = (HY_DIR / "02-seal-move-reject-nonowned-and-interior-closure.hy").read_text(encoding="utf-8")
        self.assertIn("HYB-E3004", reject)
        self.assertIn("HYB-E3015", reject)

        row9 = (HY_DIR / "03-row9-foreign-callback-flip.hy").read_text(encoding="utf-8")
        self.assertIn("HYB-E3013", row9)


class TestCorpus19Twin(unittest.TestCase):
    """P1: `move` still bans the sender read; `seal move` is the zero-copy escape."""

    def test_plain_move_then_read_stays_rejected(self) -> None:
        agg = Binding(name="agg", type=map_type())
        with self.assertRaises(SealError) as ctx:
            move_then_read(agg)
        self.assertEqual(ctx.exception.code, "HYB-E3003")

    def test_seal_move_twin_accepts_and_is_readable_by_sender_and_receivers(self) -> None:
        agg = Binding(name="agg", type=map_type())
        witness = seal_move_twin(agg, live_bindings={"agg": agg})
        self.assertEqual(witness, "sealed(agg)")


class TestExclusivityAndOwnership(unittest.TestCase):
    """HYB-E3004: seal move on a non-owned parameter/capture is rejected."""

    def test_non_owned_binding_rejected(self) -> None:
        routes = Binding(name="routes", type=map_type(), owned=False)
        with self.assertRaises(SealError) as ctx:
            seal_move(routes, live_bindings={"routes": routes})
        self.assertEqual(ctx.exception.code, "HYB-E3004")

    def test_owned_binding_with_no_live_aliases_accepted(self) -> None:
        agg = Binding(name="agg", type=map_type(), owned=True)
        witness = seal_move(agg, live_bindings={"agg": agg})
        self.assertEqual(witness, "sealed(agg)")


class TestF2StorageComponentExclusivity(unittest.TestCase):
    """F2 (declared falsifier) + R2 remedy: a pre-seal sub-slice/alias that
    outlives the seal must be caught *statically*, because
    work/sealed_test.go's TestF2_PreSealSubsliceAliasSurvivesSealAndStillMutates
    shows the runtime shape alone cannot stop a write through it."""

    def test_live_alias_in_storage_component_rejected(self) -> None:
        slice_type = TypeDesc(kind="slice", fields=(TypeDesc(kind="int"),))
        buf = Binding(name="buf", type=slice_type, aliases=frozenset({"leaked"}))
        leaked = Binding(name="leaked", type=slice_type, aliases=frozenset({"buf"}))

        live = {"buf": buf, "leaked": leaked}
        with self.assertRaises(SealError) as ctx:
            seal_move(buf, live_bindings=live)
        self.assertEqual(ctx.exception.code, "HYB-E3003")
        self.assertIn("leaked", ctx.exception.message)

    def test_alias_going_out_of_scope_before_seal_is_accepted(self) -> None:
        buf = Binding(name="buf", type=TypeDesc(kind="slice", fields=(TypeDesc(kind="int"),)), aliases=frozenset({"leaked"}))
        # `leaked` is no longer live (e.g. its lexical scope ended) by the
        # time `buf` is sealed — only entries present in live_bindings count.
        witness = seal_move(buf, live_bindings={"buf": buf})
        self.assertEqual(witness, "sealed(buf)")


class TestF3InteriorSealability(unittest.TestCase):
    """F3 (declared falsifier) + R3 remedy: recursive representation purity."""

    def test_closure_field_is_not_sealable(self) -> None:
        cfg_type = TypeDesc(kind="struct", fields=(TypeDesc(kind="func"),))
        self.assertFalse(is_sealable(cfg_type))

        cfg = Binding(name="cfg", type=cfg_type)
        with self.assertRaises(SealError) as ctx:
            seal_move(cfg, live_bindings={"cfg": cfg})
        self.assertEqual(ctx.exception.code, "HYB-E3015")

    def test_plain_data_struct_is_sealable(self) -> None:
        plain_type = TypeDesc(
            kind="struct",
            fields=(TypeDesc(kind="string"), TypeDesc(kind="slice", fields=(TypeDesc(kind="int"),))),
        )
        self.assertTrue(is_sealable(plain_type))

    def test_nested_chan_is_not_sealable(self) -> None:
        nested = TypeDesc(kind="struct", fields=(TypeDesc(kind="slice", fields=(TypeDesc(kind="chan"),)),))
        self.assertFalse(is_sealable(nested))


class TestRow9ForeignCallbackFlip(unittest.TestCase):
    """P3: GUARANTEES.md Rows 5 & 9 — const or sealed is required at a
    foreign-callback capture boundary; anything else is HYB-E3013."""

    def test_const_capture_accepted_without_sealing(self) -> None:
        cfg = Binding(name="cfg", type=map_type(), is_const=True)
        result = foreign_callback_capture(cfg, live_bindings={"cfg": cfg})
        self.assertTrue(result.startswith("ACCEPT (Class 4 const capture"))

    def test_sealable_nonconst_capture_accepted_as_sealed(self) -> None:
        routes = Binding(name="routes", type=map_type(), owned=True, is_const=False)
        result = foreign_callback_capture(routes, live_bindings={"routes": routes})
        self.assertTrue(result.startswith("ACCEPT (sealed capture"))

    def test_nonowned_nonconst_capture_rejected_with_e3013(self) -> None:
        routes = Binding(name="routes", type=map_type(), owned=False, is_const=False)
        with self.assertRaises(SealError) as ctx:
            foreign_callback_capture(routes, live_bindings={"routes": routes})
        self.assertEqual(ctx.exception.code, "HYB-E3013")


class TestGoRuntimeSpike(unittest.TestCase):
    """Drives the Go half of the spike: SealedMap/SealedSlice runtime shape,
    the P1/P3 positive hypotheses, and the F1/F3 runtime falsifiers."""

    def test_go_vet_clean(self) -> None:
        res = subprocess.run(["go", "vet", "./..."], cwd=str(WORK_DIR), capture_output=True, text=True, timeout=120)
        self.assertEqual(res.returncode, 0, f"go vet failed:\n{res.stdout}\n{res.stderr}")

    def test_go_test_suite_passes(self) -> None:
        res = subprocess.run(
            ["go", "test", "-v", "-count=1", "./..."],
            cwd=str(WORK_DIR),
            capture_output=True,
            text=True,
            timeout=120,
        )
        self.assertEqual(res.returncode, 0, f"go test failed:\n{res.stdout}\n{res.stderr}")
        for name in (
            "TestP1_SealTransitionBroadcastsAcrossWorkersSenderStillReads",
            "TestP1_SealedTypesExposeNoMutationMethods",
            "TestF1_NaiveUnclippedSealSliceAppendSpareCapacityRace",
            "TestF1Remedy_ClippedSealSliceAppendNeverAliases",
            "TestF2_PreSealSubsliceAliasSurvivesSealAndStillMutates",
            "TestF3_InteriorClosureLaundersMutationThroughNaiveSeal",
            "TestF3Remedy_SealValueRejectsInteriorClosure",
            "TestP3_SealedForeignCallbackCaptureHasNoWritePath",
        ):
            self.assertIn(f"--- PASS: {name}", res.stdout)


if __name__ == "__main__":
    unittest.main()
