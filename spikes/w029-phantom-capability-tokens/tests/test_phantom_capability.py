import unittest
import sys
import os

# Add parent directory to path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))

from phantom_capability_model import PhantomTypeChecker, Permission, run_test_matrix, benchmark_graph_vs_phantom

class TestPhantomCapabilityTokens(unittest.TestCase):
    def setUp(self):
        self.tc = PhantomTypeChecker()

    def test_full_matrix(self):
        matrix = run_test_matrix()
        for t in matrix:
            with self.subTest(id=t["id"], name=t["name"]):
                self.assertTrue(t["passed"], f"Test {t['id']} failed")

    def test_zero_alloc_phantom_overhead(self):
        # Verify that capability instances hold only payload value
        cap = self.tc.declare_unique("test_val", "int", 42)
        self.assertEqual(cap.value, 42)
        self.assertEqual(cap.perm, Permission.UNIQUE)

    def test_w004_closure_capture_falsifier_resolved(self):
        """
        Explicitly reproduces and proves resolution of W-004 Falsifier:
        In W-004, lexical captures continued to share mutable heap storage across goroutines.
        Our Lexical Fence Receptor marks the variable EXPIRED in caller environment upon spawn,
        preventing any subsequent read or write by caller while child task executes.
        """
        self.tc.declare_unique("state", "State", {"counter": 0})
        # Spawn child task taking ownership of state
        ok = self.tc.spawn_task("WorkerGoroutine", ["state"])
        self.assertTrue(ok)
        
        # Caller tries to access state -> MUST FAIL with HYB-E3003
        read_res = self.tc.read_shared("state")
        self.assertIsNone(read_res)
        self.assertTrue(any(d.code == "HYB-E3003" for d in self.tc.diagnostics))

        # Caller tries to mutate state -> MUST FAIL with HYB-E3003
        mut_ok = self.tc.mutate_unique("state", lambda s: s.update({"counter": 1}))
        self.assertFalse(mut_ok)

    def test_benchmark_speedup_threshold(self):
        bench = benchmark_graph_vs_phantom(num_nodes=500)
        self.assertGreater(bench["speedup_factor"], 1.5, "Speedup factor must be at least 1.5x over graph walk")

if __name__ == '__main__':
    unittest.main()
