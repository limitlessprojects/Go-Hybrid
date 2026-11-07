import unittest
import os
import subprocess
import json

SPIKE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VERIFIER_PATH = os.path.join(SPIKE_DIR, "sync_guarded_verifier.py")
HY_DIR = os.path.join(SPIKE_DIR, "hy")

class TestSyncGuardedSpike(unittest.TestCase):

    def run_verifier(self, filename):
        filepath = os.path.join(HY_DIR, filename)
        cmd = ["python3", VERIFIER_PATH, filepath, "--json"]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        self.assertTrue(proc.stdout, f"No stdout from verifier: {proc.stderr}")
        data = json.loads(proc.stdout)
        return proc.returncode, data

    def test_01_clean_sync_guarded_accepts(self):
        rc, data = self.run_verifier("01-clean-sync-guarded.hy")
        self.assertEqual(rc, 0)
        self.assertEqual(data["verdict"], "ACCEPT")
        self.assertEqual(len(data["diagnostics"]), 0)

    def test_02_unguarded_access_rejects_e3003(self):
        rc, data = self.run_verifier("02-unguarded-access-reject.hy")
        self.assertEqual(rc, 1)
        self.assertEqual(data["verdict"], "REJECT")
        codes = [d["code"] for d in data["diagnostics"]]
        self.assertIn("HYB-E3003", codes)

    def test_03_nonexistent_mutex_rejects_e3002(self):
        rc, data = self.run_verifier("03-nonexistent-mutex-reject.hy")
        self.assertEqual(rc, 1)
        self.assertEqual(data["verdict"], "REJECT")
        codes = [d["code"] for d in data["diagnostics"]]
        self.assertIn("HYB-E3002", codes)

    def test_04_unannotated_mutable_rejects_e3001(self):
        rc, data = self.run_verifier("04-unannotated-mutable-reject.hy")
        self.assertEqual(rc, 1)
        self.assertEqual(data["verdict"], "REJECT")
        codes = [d["code"] for d in data["diagnostics"]]
        self.assertIn("HYB-E3001", codes)

    def test_05_early_unlock_escape_rejects_e3003(self):
        rc, data = self.run_verifier("05-early-unlock-escape-reject.hy")
        self.assertEqual(rc, 1)
        self.assertEqual(data["verdict"], "REJECT")
        codes = [d["code"] for d in data["diagnostics"]]
        self.assertIn("HYB-E3003", codes)

if __name__ == "__main__":
    unittest.main()
