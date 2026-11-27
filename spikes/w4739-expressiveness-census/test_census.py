import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import census


class EvidenceTests(unittest.TestCase):
    def test_identical_source_has_zero_tax(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / "a.hy", Path(directory) / "b.hy"
            a.write_text("fn main() {\n    x := 1\n}\n", encoding="utf-8")
            b.write_bytes(a.read_bytes())
            self.assertEqual((0, 0), census.diff_counts(a, b))

    def test_one_replacement_is_one_addition_and_deletion(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / "a.hy", Path(directory) / "b.hy"
            a.write_text("first\nold\nlast\n", encoding="utf-8")
            b.write_text("first\nnew\nlast\n", encoding="utf-8")
            self.assertEqual((1, 1), census.diff_counts(a, b))

    def checker_result(self, exit_code, verdict, errors):
        return {"exit": exit_code, "stdout": json.dumps({
            "diagnostics": [{"severity": "error", "code": "HYB-E3001"}] * errors,
            "summary": {"verdict": verdict, "errors": errors}}), "stderr": ""}

    def test_crashing_checker_is_not_a_rejection(self):
        with patch.object(census, "run", return_value={"exit": 2, "stdout": "", "stderr": "crash"}):
            with self.assertRaisesRegex(RuntimeError, "infrastructure"):
                census.check("hyb", "source.hy")

    def test_inconsistent_exit_and_json_are_rejected(self):
        with patch.object(census, "run", return_value=self.checker_result(0, "reject", 1)):
            with self.assertRaises(RuntimeError):
                census.check("hyb", "source.hy")

    def test_reject_without_errors_is_not_valid_evidence(self):
        with patch.object(census, "run", return_value=self.checker_result(1, "reject", 0)):
            with self.assertRaises(RuntimeError):
                census.check("hyb", "source.hy")

    def test_malformed_json_is_not_accepted(self):
        with patch.object(census, "run", return_value={"exit": 0, "stdout": "not JSON"}):
            with self.assertRaises(ValueError):
                census.check("hyb", "source.hy")

    def test_failing_behavior_blocks_acceptance(self):
        checker = {"summary": {"verdict": "accept", "managed": {"managed_blocks": 2}}}
        self.assertEqual("NOT_VERIFIED", census.classify(checker, False, []))

    def test_escape_prevents_clean_classification(self):
        checker = {"summary": {"verdict": "accept", "managed": {"managed_blocks": 0}}}
        self.assertEqual("ESCAPED", census.classify(checker, True, ["Go helper"]))
        checker["summary"]["managed"]["managed_blocks"] = 1
        self.assertEqual("ESCAPED", census.classify(checker, True, []))

    def test_absent_managed_report_is_not_zero(self):
        with self.assertRaises(RuntimeError):
            census.classify({"summary": {"verdict": "accept"}}, True, [])

    def test_empty_test_log_is_not_a_pass(self):
        with patch.object(census, "run", return_value={"exit": 0, "stdout": "", "stderr": ""}):
            with self.assertRaisesRegex(RuntimeError, "behavioral"):
                census.go_test("go", Path("."), ["./errgroup"], 1)

    def test_frozen_input_tampering_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "naive.hy"
            source.write_text("frozen", encoding="utf-8")
            (root / "inputs.json").write_text(json.dumps({"sha256": {"naive.hy": census.digest(source)}}))
            with patch.object(census, "ROOT", root):
                census.verify_inputs()
                source.write_text("changed", encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "changed"):
                    census.verify_inputs()


if __name__ == "__main__":
    unittest.main()
