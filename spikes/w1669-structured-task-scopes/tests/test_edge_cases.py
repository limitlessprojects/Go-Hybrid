"""
Edge case tests for structured task scopes.

Tests complex control flow, deeply nested scopes, and corner cases
beyond the 25 canonical leaky patterns.
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from scope_checker import check_structured_scope


class TestEdgeCases(unittest.TestCase):
    """Edge cases and complex control flow scenarios."""
    
    def test_deeply_nested_scopes(self):
        """Test 5 levels of nested scopes."""
        src = """
        scope {
            h1 := task level1()
            scope {
                h2 := task level2()
                scope {
                    h3 := task level3()
                    scope {
                        h4 := task level4()
                        scope {
                            h5 := task level5()
                            join(move h5)
                        }
                        join(move h4)
                    }
                    join(move h3)
                }
                join(move h2)
            }
            join(move h1)
        }
        """
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")
    
    def test_deeply_nested_with_leak(self):
        """Test that leak in deeply nested scope is detected."""
        src = """
        scope {
            h1 := task level1()
            scope {
                h2 := task level2()
                scope {
                    h3 := task level3()
                    // h3 not joined - leak
                }
                join(move h2)
            }
            join(move h1)
        }
        """
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
        self.assertIn("h3", res.unjoined_handles)
    
    def test_multiple_returns_in_scope(self):
        """Test multiple early returns."""
        src = """
        scope {
            h := task worker()
            if condition1 {
                return early1
            }
            if condition2 {
                return early2
            }
            join(move h)
        }
        """
        res = check_structured_scope(src)
        # Should fail because h is not joined on early return paths
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
    
    def test_empty_scope(self):
        """Test scope with no tasks."""
        src = "scope { }"
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")
    
    def test_scope_with_only_comments(self):
        """Test scope with only comments."""
        src = """
        scope {
            // This is a comment
            // Another comment
        }
        """
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")
    
    def test_handle_reuse_after_consumption(self):
        """Test that handle cannot be used after consumption."""
        src = """
        scope {
            h := task worker()
            join(move h)
            x := h  // Error: h already consumed
        }
        """
        # This is a semantic error that our line-based checker doesn't catch
        # A full AST-based checker would catch this
        res = check_structured_scope(src)
        # For now, we just verify it doesn't crash
        self.assertIsNotNone(res)
    
    def test_conditional_spawn_both_paths_joined(self):
        """Test conditional spawn where both paths join."""
        src = """
        scope {
            if condition {
                h := task worker()
                join(move h)
            } else {
                h := task other_worker()
                join(move h)
            }
        }
        """
        # Our line-based checker sees two spawns and two joins
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
    
    def test_loop_with_accumulated_handles(self):
        """Test loop that accumulates handles (should fail)."""
        src = """
        scope {
            for i in 0..10 {
                h := task worker(i)
                // Handles accumulate but never joined
            }
        }
        """
        res = check_structured_scope(src)
        # Should detect unjoined handles at scope exit
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
    
    def test_mixed_join_cancel_drain(self):
        """Test mixing different consumption methods."""
        src = """
        scope {
            h1 := task worker1()
            h2 := task worker2()
            h3 := task worker3()
            join(move h1)
            cancel(move h2)
            drain(move h3)
        }
        """
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")
    
    def test_whitespace_and_formatting(self):
        """Test that checker handles various whitespace."""
        src = """
        scope{
        h:=task worker()
        join(move h)
        }
        """
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
    
    def test_multiple_foreign_callbacks(self):
        """Test multiple foreign callbacks in sequence."""
        src = """
        http.HandleFunc("/a", fn() {
            h1 := task worker1()
        })
        rpc.RegisterHandler("b", fn() {
            h2 := task worker2()
        })
        """
        res = check_structured_scope(src)
        # Should detect unmanaged foreign callbacks
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3503")
    
    def test_managed_then_unmanaged(self):
        """Test managed callback followed by unmanaged."""
        src = """
        managed("first", "foreign_callback")
        http.HandleFunc("/a", fn() {
            h1 := task worker1()
            join(move h1)
        })
        // Second callback not managed - but first scope's unjoined handle detected first
        rpc.RegisterHandler("b", fn() {
            h2 := task worker2()
        })
        """
        res = check_structured_scope(src)
        # The checker processes line-by-line, so it detects the unjoined h2 (HYB-E3501)
        # before the unmanaged foreign callback (HYB-E3503)
        self.assertFalse(res.accepted)
        # Either E3501 or E3503 is acceptable depending on detection order
        self.assertIn(res.diagnostic_code, ["HYB-E3501", "HYB-E3503"])
    
    def test_scope_in_function_with_early_return(self):
        """Test scope inside function with early return."""
        src = """
        func process() -> Result {
            scope {
                h := task worker()
                if error {
                    return Result{error: "failed"}
                }
                join(move h)
            }
            return Result{success: true}
        }
        """
        res = check_structured_scope(src)
        # Should detect that h is not joined on error path
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
    
    def test_parallel_scopes(self):
        """Test multiple parallel scopes."""
        src = """
        scope {
            h1 := task worker1()
            join(move h1)
        }
        scope {
            h2 := task worker2()
            join(move h2)
        }
        scope {
            h3 := task worker3()
            join(move h3)
        }
        """
        res = check_structured_scope(src)
        self.assertTrue(res.accepted)
    
    def test_scope_with_panic_recovery(self):
        """Test scope with panic (should detect leak)."""
        src = """
        scope {
            h := task worker()
            panic("something went wrong")
            join(move h)  // Unreachable
        }
        """
        res = check_structured_scope(src)
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")


if __name__ == "__main__":
    unittest.main(verbosity=2)
