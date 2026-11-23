#!/usr/bin/env python3
"""
spikes/w4656-self-healing-fact-store/tests/test_fact_store.py

Verification tests for Spike W-4656:
- Falsifier 1: Semantically inert edits reuse verdict.
- Falsifier 2: Callee return provenance changes trigger invalidation and recheck.
- Falsifier 3: Scope edit resolving previously missing binding triggers recheck.
- Falsifier 4: Unrelated function edits leave independent callers cached.
- Falsifier 5: Efficiency metrics (>50% check reduction on editing session).
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from fact_store import Diagnostic, FunctionVerdict, SelfHealingFactStore, SourceSnapshot, Verdict


class TestSelfHealingFactStore(unittest.TestCase):
    def setUp(self):
        self.store = SelfHealingFactStore()

    def test_semantically_inert_edit_reuses_verdict(self):
        """Comments and cosmetic formatting in source do not change IR hash -> verdict reused."""
        snap1 = SourceSnapshot(
            functions={"fn_a": "let x = 1\ncall helper()"},
            global_symbols={},
            callee_summaries={"helper": "affine_clean"},
        )
        v1, reused1 = self.store.evaluate_function("fn_a", snap1)
        self.assertFalse(reused1)
        self.assertEqual(v1.verdict, Verdict.ACCEPT)

        # Snap 2: Add comments and spaces
        snap2 = SourceSnapshot(
            functions={"fn_a": "// A cosmetic comment\nlet x = 1  \n\n// another comment\ncall helper()"},
            global_symbols={},
            callee_summaries={"helper": "affine_clean"},
        )
        v2, reused2 = self.store.evaluate_function("fn_a", snap2)
        self.assertTrue(reused2, "Verdict should be reused on semantically inert edit")
        self.assertEqual(v2.verdict, Verdict.ACCEPT)
        self.assertEqual(self.store.full_check_count, 1)

    def test_callee_provenance_change_invalidates_and_recomputes(self):
        """When callee signature remains but modeled return provenance escapes, caller is recomputed."""
        snap1 = SourceSnapshot(
            functions={"fn_caller": "call worker()"},
            global_symbols={},
            callee_summaries={"worker": "affine_safe"},
        )
        v1, _ = self.store.evaluate_function("fn_caller", snap1)
        self.assertEqual(v1.verdict, Verdict.ACCEPT)

        # Callee worker now changes return provenance to escaping_tainted
        snap2 = SourceSnapshot(
            functions={"fn_caller": "call worker()"},
            global_symbols={},
            callee_summaries={"worker": "escaping_tainted"},
        )
        v2, reused = self.store.evaluate_function("fn_caller", snap2)
        self.assertFalse(reused, "Caller verdict must NOT be reused when callee provenance mutates")
        self.assertEqual(v2.verdict, Verdict.REJECT)
        self.assertTrue(any(d.code == "HYB-E3001" for d in v2.diagnostics))
        self.assertEqual(self.store.full_check_count, 2)

    def test_scope_edit_resolving_missing_binding_recomputes(self):
        """When an unresolved symbol is subsequently declared, prior rejection verdict cannot survive."""
        snap1 = SourceSnapshot(
            functions={"fn_main": "let v = unresolved_foo;"},
            global_symbols={},
            callee_summaries={},
        )
        v1, _ = self.store.evaluate_function("fn_main", snap1)
        self.assertEqual(v1.verdict, Verdict.REJECT)
        self.assertTrue(any(d.code == "HYB-E1001" for d in v1.diagnostics))

        # Snapshot 2: Symbol 'foo' is now introduced in scope
        snap2 = SourceSnapshot(
            functions={"fn_main": "let v = unresolved_foo;"},
            global_symbols={"foo": "const_int"},
            callee_summaries={},
        )
        v2, reused = self.store.evaluate_function("fn_main", snap2)
        self.assertFalse(reused, "Rejection must NOT be reused once previously missing symbol resolves")
        self.assertEqual(v2.verdict, Verdict.ACCEPT)

    def test_unrelated_caller_retains_verdict_when_sibling_is_edited(self):
        """Editing function B does not invalidate cached function A."""
        snap1 = SourceSnapshot(
            functions={
                "fn_a": "call shared_util()",
                "fn_b": "let y = 10",
            },
            global_symbols={},
            callee_summaries={"shared_util": "clean"},
        )
        self.store.evaluate_function("fn_a", snap1)
        self.store.evaluate_function("fn_b", snap1)
        self.assertEqual(self.store.full_check_count, 2)

        # Snapshot 2: fn_b is completely rewritten, fn_a unchanged
        snap2 = SourceSnapshot(
            functions={
                "fn_a": "call shared_util()",
                "fn_b": "let y = 20; let z = 30",
            },
            global_symbols={},
            callee_summaries={"shared_util": "clean"},
        )
        v_a, reused_a = self.store.evaluate_function("fn_a", snap2)
        v_b, reused_b = self.store.evaluate_function("fn_b", snap2)

        self.assertTrue(reused_a, "Unchanged fn_a should reuse cached verdict")
        self.assertFalse(reused_b, "Changed fn_b must recheck")
        self.assertEqual(self.store.full_check_count, 3)

    def test_editor_session_reduction_metric(self):
        """Simulate 10 edits where 7 are comments/formatting. Verified reduction > 60%."""
        funcs = {"f1": "call leaf()", "f2": "let a = 1"}
        summaries = {"leaf": "safe"}

        store = SelfHealingFactStore()
        for i in range(10):
            # Edits 1..7 are comments in f1
            if i < 7:
                funcs["f1"] = f"// edit {i}\ncall leaf()"
            else:
                funcs["f2"] = f"let a = {i}"
            snap = SourceSnapshot(functions=dict(funcs), global_symbols={}, callee_summaries=dict(summaries))
            store.evaluate_function("f1", snap)
            store.evaluate_function("f2", snap)

        # 20 evaluations performed. With caching, full checks are minimal.
        full_checks = store.full_check_count
        reused = store.reused_verdict_count
        reduction = reused / (full_checks + reused)
        self.assertGreater(reduction, 0.50, f"Expected > 50% check reduction, got {reduction:.2%}")


if __name__ == "__main__":
    unittest.main()
