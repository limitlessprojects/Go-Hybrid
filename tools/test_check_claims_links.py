import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOL = Path(__file__).resolve().parent / "check_claims_links.py"

CLAIMS = """# GUARANTEES

## The claims table

| # | Surface | What the checker does | The claim | Evidence |
|---|---------|----------------------|-----------|----------|
| 1 | spawn | every argument classified | **Guaranteed** | [spec](SPEC-LANGUAGE.md); corpus [01](tests/checker-corpus/01-bad.hy) (reject) / [15](tests/checker-corpus/15-good.hy) (accept); corpus `34-select-send-unproven-name`; corpus 16 |
| 2 | send | same classification | **Guaranteed** | [external](https://example.com/spec), [anchor](#row-2); corpus [16](tests/checker-corpus/16-move.hy) |
"""


class CheckClaimsLinksTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "GUARANTEES.md").write_text(CLAIMS, encoding="utf-8")
        (self.root / "SPEC-LANGUAGE.md").write_text("# spec\n", encoding="utf-8")
        corpus = self.root / "tests" / "checker-corpus"
        corpus.mkdir(parents=True)
        (corpus / "01-bad.hy").write_text("// EXPECTED-VERDICT: REJECT A1c\nfn main() {}\n", encoding="utf-8")
        (corpus / "15-good.hy").write_text("// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8")
        (corpus / "16-move.hy").write_text("// EXPECTED-VERDICT: ACCEPT\n", encoding="utf-8")
        (corpus / "34-select-send-unproven-name.hy").write_text("// EXPECTED-VERDICT: REJECT E3005\n", encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def run_tool(self, *extra):
        return subprocess.run(
            [sys.executable, str(TOOL), "--root", str(self.root), "--json", *extra],
            capture_output=True, text=True,
        )

    def test_valid_table_passes(self):
        result = self.run_tool()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        report = json.loads(result.stdout)
        self.assertTrue(report["ok"])
        self.assertEqual(report["rows"], 2)
        # SPEC-LANGUAGE.md + three corpus files; http and anchor links are skipped.
        self.assertEqual(report["links"], 4)
        self.assertEqual(report["refs"], 2)
        self.assertEqual(report["issues"], [])

    def test_missing_file_is_reported(self):
        (self.root / "tests" / "checker-corpus" / "15-good.hy").unlink()
        result = self.run_tool()
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual([i["kind"] for i in report["issues"]], ["missing"])
        self.assertIn("15-good.hy", report["issues"][0]["target"])

    def test_verdict_mismatch_is_reported(self):
        (self.root / "tests" / "checker-corpus" / "15-good.hy").write_text(
            "// EXPECTED-VERDICT: REJECT A1c\n", encoding="utf-8")
        result = self.run_tool()
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        mismatch = [i for i in report["issues"] if i["kind"] == "verdict-mismatch"]
        self.assertEqual(len(mismatch), 1)
        self.assertEqual(mismatch[0]["row"], 1)
        self.assertEqual(mismatch[0]["expected"], "ACCEPT")
        self.assertEqual(mismatch[0]["found"], "REJECT")

    def test_missing_verdict_header_is_reported(self):
        (self.root / "tests" / "checker-corpus" / "01-bad.hy").write_text("fn main() {}\n", encoding="utf-8")
        result = self.run_tool()
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual([i["kind"] for i in report["issues"]], ["no-verdict-header"])

    def test_missing_inline_corpus_is_reported(self):
        (self.root / "tests" / "checker-corpus" / "34-select-send-unproven-name.hy").unlink()
        result = self.run_tool()
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        kinds = [i["kind"] for i in report["issues"]]
        self.assertIn("missing-inline-corpus", kinds)

    def test_missing_corpus_number_is_reported(self):
        (self.root / "tests" / "checker-corpus" / "16-move.hy").unlink()
        result = self.run_tool()
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        kinds = [i["kind"] for i in report["issues"]]
        self.assertIn("missing-corpus-number", kinds)

    def test_missing_claims_file_is_reported(self):
        (self.root / "GUARANTEES.md").unlink()
        result = self.run_tool()
        self.assertEqual(result.returncode, 1)
        report = json.loads(result.stdout)
        self.assertEqual(report["issues"][0]["kind"], "missing-claims-file")


if __name__ == "__main__":
    unittest.main()
