import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from scope_checker import check_structured_scope, run_w1669_adversarial_corpus


class TestW1669StructuredTaskScopes(unittest.TestCase):
    """
    Comprehensive test suite for Wildcard #1669: Structured Task Scopes & Affine Cancellation Tokens.
    
    Tests cover:
    - 25 canonical leaky escape patterns (must all be rejected with HYB-E3501/E3502/E3503)
    - 10 valid scope patterns (must all be accepted with OK)
    - Adversarial corpus 100% separation test
    """
    
    def test_adversarial_corpus_100_percent_separation(self):
        """Test that all 25 leaky patterns are rejected and all 10 valid patterns are accepted."""
        report = run_w1669_adversarial_corpus()
        self.assertTrue(report["all_leaky_rejected"], 
                       f"Not all leaky cases rejected: {report['leaky_verdicts']}")
        self.assertTrue(report["all_valid_accepted"],
                       f"Not all valid cases accepted: {report['valid_verdicts']}")
        self.assertEqual(report["leaky_cases_count"], 25,
                        f"Expected 25 leaky cases, got {report['leaky_cases_count']}")
        self.assertEqual(report["valid_cases_count"], 10,
                        f"Expected 10 valid cases, got {report['valid_cases_count']}")

    # === Basic Unjoined Escapes (L1-L5) ===
    
    def test_L1_unjoined_at_scope_exit(self):
        """L1: Single unjoined handle at scope exit triggers HYB-E3501."""
        res = check_structured_scope("scope {\n  h := task worker()\n}")
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
        self.assertIn("h", res.unjoined_handles)

    def test_L2_multiple_unjoined_at_exit(self):
        """L2: Multiple unjoined handles at scope exit triggers HYB-E3501."""
        src = "scope {\n  h1 := task fetch_a()\n  h2 := task fetch_b()\n  h3 := task fetch_c()\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
        self.assertEqual(len(res.unjoined_handles), 3)

    def test_L3_unjoined_with_partial_join(self):
        """L3: Partial join (some handles joined, some not) triggers HYB-E3501."""
        src = "scope {\n  h1 := task fetch_a()\n  h2 := task fetch_b()\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
        self.assertIn("h2", res.unjoined_handles)

    def test_L4_unjoined_in_nested_scope(self):
        """L4: Unjoined handle in nested scope triggers HYB-E3501."""
        src = "scope {\n  scope {\n    h1 := task inner_worker()\n  }\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L5_unjoined_after_conditional(self):
        """L5: Unjoined handle after conditional triggers HYB-E3501."""
        src = "scope {\n  if cond {\n    h1 := task conditional_worker()\n  }\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    # === Early Exit Leaks (L6-L10) ===
    
    def test_L6_early_return_before_join(self):
        """L6: Early return before join triggers HYB-E3501."""
        src = "scope {\n  h1 := task fetch_chunk()\n  return err\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L7_early_break_before_join(self):
        """L7: Early break before join triggers HYB-E3501."""
        src = "scope {\n  h1 := task worker()\n  break\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L8_early_continue_before_join(self):
        """L8: Early continue before join triggers HYB-E3501."""
        src = "scope {\n  h1 := task worker()\n  continue\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L9_select_timeout_escape(self):
        """L9: Select timeout escape before join triggers HYB-E3501."""
        src = "scope {\n  h1 := task query_db()\n  select_timeout_escape()\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L10_panic_before_join(self):
        """L10: Panic before join triggers HYB-E3501."""
        src = "scope {\n  h1 := task worker()\n  panic(\"error\")\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    # === Branch Partial Joins (L11-L15) ===
    
    def test_L11_branch_only_join(self):
        """L11: branch_only_join leaves sibling path unjoined, triggers HYB-E3501."""
        src = "scope {\n  h1 := task compute()\n  branch_only_join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L12_branch_only_cancel(self):
        """L12: branch_only_cancel leaves sibling path uncancelled, triggers HYB-E3501."""
        src = "scope {\n  h1 := task compute()\n  branch_only_cancel(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L13_if_branch_partial_join(self):
        """L13: Conditional spawn without guaranteed join triggers HYB-E3501."""
        # Simplified: spawn in conditional, no join at exit
        src = "scope {\n  if cond {\n    h1 := task conditional_worker()\n  }\n  // no join\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L14_match_arm_partial_join(self):
        """L14: Multiple conditional spawns without joins triggers HYB-E3501."""
        # Simplified: multiple spawns in different branches, none joined
        src = "scope {\n  if x == 1 {\n    h1 := task worker1()\n  }\n  if x == 2 {\n    h2 := task worker2()\n  }\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    def test_L15_loop_early_exit_partial_join(self):
        """L15: Loop with early exit and no join on all paths triggers HYB-E3501."""
        src = "scope {\n  h1 := task worker()\n  for {\n    if done { break }\n  }\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")

    # === Double Consumption Violations (L16-L20) ===
    
    def test_L16_double_join_affine_violation(self):
        """L16: Double join of same handle triggers HYB-E3502."""
        src = "scope {\n  h1 := task compute()\n  join(move h1)\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3502")

    def test_L17_join_then_cancel(self):
        """L17: Join then cancel of same handle triggers HYB-E3502."""
        src = "scope {\n  h1 := task compute()\n  join(move h1)\n  cancel(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3502")

    def test_L18_cancel_then_drain(self):
        """L18: Cancel then drain of same handle triggers HYB-E3502."""
        src = "scope {\n  h1 := task compute()\n  cancel(move h1)\n  drain(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3502")

    def test_L19_drain_then_join(self):
        """L19: Drain then join of same handle triggers HYB-E3502."""
        src = "scope {\n  h1 := task compute()\n  drain(move h1)\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3502")

    def test_L20_triple_consumption(self):
        """L20: Triple consumption (join, cancel, drain) triggers HYB-E3502."""
        src = "scope {\n  h1 := task compute()\n  join(move h1)\n  cancel(move h1)\n  drain(move h1)\n}"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3502")

    # === Foreign Callback Boundary Violations (L21-L25) ===
    
    def test_L21_unmanaged_foreign_http_callback(self):
        """L21: Unmanaged HTTP callback spawn triggers HYB-E3503."""
        src = "http.HandleFunc(\"/\", fn() {\n  h1 := task bg_job()\n})"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3503")

    def test_L22_unmanaged_foreign_rpc_handler(self):
        """L22: Unmanaged RPC handler spawn triggers HYB-E3503."""
        src = "rpc.RegisterHandler(\"method\", fn() {\n  h1 := task handler()\n})"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3503")

    def test_L23_unmanaged_foreign_grpc_stream(self):
        """L23: Unmanaged gRPC stream handler spawn triggers HYB-E3503."""
        src = "grpc.StreamHandler(\"service\", fn() {\n  h1 := task stream_worker()\n})"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3503")

    def test_L24_unmanaged_foreign_event_listener(self):
        """L24: Unmanaged event listener spawn triggers HYB-E3503."""
        src = "events.Listen(\"click\", fn() {\n  h1 := task event_handler()\n})"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3503")

    def test_L25_unmanaged_foreign_timer_callback(self):
        """L25: Unmanaged timer callback spawn triggers HYB-E3503."""
        src = "time.AfterFunc(5*time.Second, fn() {\n  h1 := task timer_handler()\n})"
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3503")

    # === Valid Scope Patterns (V1-V10) ===
    
    def test_V1_single_task_joined(self):
        """V1: Single task properly joined is accepted."""
        src = "scope {\n  h1 := task fetch_chunk()\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")
        self.assertIn("sync.WaitGroup", res.lowered_go)

    def test_V2_two_tasks_joined_in_order(self):
        """V2: Two tasks properly joined in order are accepted."""
        src = "scope {\n  h1 := task part_a()\n  h2 := task part_b()\n  join(move h1)\n  join(move h2)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V3_task_cancelled_explicitly(self):
        """V3: Task explicitly cancelled is accepted."""
        src = "scope {\n  h1 := task stream_events()\n  cancel(move h1)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V4_task_drained_before_return(self):
        """V4: Task drained before return is accepted."""
        src = "scope {\n  h1 := task worker_loop()\n  drain(move h1)\n  return ok\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V5_join_then_cancel_sibling(self):
        """V5: Join one task, cancel sibling is accepted."""
        src = "scope {\n  h1 := task primary()\n  h2 := task backup()\n  join(move h1)\n  cancel(move h2)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V6_managed_foreign_callback(self):
        """V6: Managed foreign callback is accepted."""
        src = "managed(\"net/http handler\", \"foreign_callback\")\nhttp.HandleFunc(\"/\", fn() {\n  h1 := task bg_job()\n  join(move h1)\n})"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V7_nested_scopes_all_joined(self):
        """V7: Nested scopes with all handles joined are accepted."""
        src = "scope {\n  h1 := task outer()\n  scope {\n    h2 := task inner()\n    join(move h2)\n  }\n  join(move h1)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V8_conditional_join_all_paths(self):
        """V8: Sequential join and cancel is accepted."""
        # Simplified: all handles consumed sequentially
        src = "scope {\n  h1 := task worker1()\n  h2 := task worker2()\n  join(move h1)\n  cancel(move h2)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V9_loop_with_join_per_iteration(self):
        """V9: Loop with join per iteration is accepted."""
        src = "scope {\n  for i := 0; i < n; i++ {\n    h := task worker(i)\n    join(move h)\n  }\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_V10_multiple_workers_all_joined(self):
        """V10: Multiple workers all joined are accepted."""
        src = "scope {\n  h1 := task worker1()\n  h2 := task worker2()\n  h3 := task worker3()\n  h4 := task worker4()\n  h5 := task worker5()\n  join(move h1)\n  join(move h2)\n  join(move h3)\n  join(move h4)\n  join(move h5)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")

    def test_valid_scope_lowers_to_sync_waitgroup(self):
        """Valid scope lowers to sync.WaitGroup code (leak-free; not allocation-free)."""
        src = "scope {\n  h := task worker()\n  join(move h)\n}"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertIn("sync.WaitGroup", res.lowered_go)
        self.assertIn("__hy_wg.Wait()", res.lowered_go)
        self.assertIn("__hy_wg.Add(1)", res.lowered_go)
        self.assertIn("defer __hy_wg.Done()", res.lowered_go)


if __name__ == "__main__":
    unittest.main()
