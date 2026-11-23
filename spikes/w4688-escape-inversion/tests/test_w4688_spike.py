import unittest
import sys
import os

# Add parent directory to path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from escape_inversion import (
    EscapeInversionPipeline,
    PlaceWitness,
    BoundaryInvocation,
    BoundaryKind,
    StorageClass,
)


class TestW4688EscapeInversion(unittest.TestCase):
    def setUp(self):
        self.pipeline = EscapeInversionPipeline()

    def test_lexical_borrow_lowering_noescape(self):
        """A lexical borrow passed to an FFI call is stack-pinned and receives //go:noescape."""
        buf = PlaceWitness(
            name="buf",
            type_name="byte",
            decl_depth=1,
            is_borrow=True,
            escapes_lexical_scope=False,
        )
        inv = BoundaryInvocation(
            kind=BoundaryKind.FFI_CALL,
            target_fn="hyrt_encode_bytes",
            call_depth=2,
            arguments=[buf],
        )

        res = self.pipeline.emit_go_lowering(inv)
        self.assertEqual(res["stack_pinned_count"], 1)
        self.assertIn("//go:noescape", res["go_source"])
        self.assertIn("TEXT ·hyrt_encode_bytes(SB), NOSPLIT", res["asm_stub"])

    def test_escaping_variable_rejected_from_stack_pinning(self):
        """A variable that escapes lexical scope is marked HEAP_ESCAPING with diagnostic HYB-E4688-01."""
        escaping_val = PlaceWitness(
            name="payload",
            type_name="Payload",
            decl_depth=1,
            escapes_lexical_scope=True,
        )
        inv = BoundaryInvocation(
            kind=BoundaryKind.CLOSURE_CALL,
            target_fn="dispatch_worker",
            call_depth=2,
            arguments=[escaping_val],
        )

        sclass = self.pipeline.classify_place(escaping_val, inv)
        self.assertEqual(sclass, StorageClass.HEAP_ESCAPING)
        self.assertTrue(any(code == "HYB-E4688-01" for code, _ in self.pipeline.diagnostics))

    def test_task_spawn_without_region_rejected(self):
        """Spawning an async task with captured places without a bounded region anchor triggers HYB-E4688-02."""
        task_data = PlaceWitness(
            name="state",
            type_name="State",
            decl_depth=1,
            escapes_lexical_scope=False,
            pinned_to_region=None,
        )
        inv = BoundaryInvocation(
            kind=BoundaryKind.TASK_SPAWN,
            target_fn="async_worker",
            call_depth=2,
            arguments=[task_data],
            region_scope=None,  # No enclosing nursery / region
        )

        sclass = self.pipeline.classify_place(task_data, inv)
        self.assertEqual(sclass, StorageClass.HEAP_ESCAPING)
        self.assertTrue(any(code == "HYB-E4688-02" for code, _ in self.pipeline.diagnostics))

    def test_task_spawn_within_bounded_region_pinned(self):
        """Spawning a task inside a certified lexical region allows stack-pinned execution."""
        task_data = PlaceWitness(
            name="state",
            type_name="State",
            decl_depth=1,
            escapes_lexical_scope=False,
            pinned_to_region="region_nursery_01",
        )
        inv = BoundaryInvocation(
            kind=BoundaryKind.TASK_SPAWN,
            target_fn="scoped_worker",
            call_depth=2,
            arguments=[task_data],
            region_scope="region_nursery_01",
        )

        sclass = self.pipeline.classify_place(task_data, inv)
        self.assertEqual(sclass, StorageClass.STACK_PINNED)

    def test_zero_allocation_parity_demonstration(self):
        """Verifies that 1,000,000 iterations achieve exactly 0 heap allocations under escape inversion."""
        arg1 = PlaceWitness(name="data", type_name="Data", decl_depth=1, is_borrow=True, escapes_lexical_scope=False)
        inv = BoundaryInvocation(
            kind=BoundaryKind.FFI_CALL,
            target_fn="leaf_compute",
            call_depth=2,
            arguments=[arg1],
        )

        metrics = self.pipeline.simulate_allocations(inv, iterations=1_000_000)
        self.assertEqual(metrics["inverted_allocs"], 0)
        self.assertEqual(metrics["inverted_bytes"], 0)
        self.assertEqual(metrics["saved_allocs"], 1_000_000)
        self.assertEqual(metrics["saved_bytes"], 24_000_000)


if __name__ == "__main__":
    unittest.main()
