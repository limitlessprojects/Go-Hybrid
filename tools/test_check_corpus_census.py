"""Tests for tools/check_corpus_census.py (stdlib only, no network).

The guard exists because the corpus size is restated in several places and
they drift apart whenever a corpus PR misses one (PR #389 repaired this once;
the drift was back on session-22's main). These tests build throwaway trees
so the drift and the clean case are both pinned.

STATUS.md is deliberately outside the surface list (supervisor-only file in
this pipeline), so no drift test exists for it.
"""
import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check_corpus_census.py")
SPEC = importlib.util.spec_from_file_location("corpus_census", SCRIPT)
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


def make_tree(files):
    tmp = tempfile.TemporaryDirectory(prefix="census-")
    root = Path(tmp.name)
    for rel, content in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    return tmp, root


def corpus_file(verdict):
    return f"// EXPECTED-VERDICT: {verdict}\nfn main() {{\n}}\n"


def consistent_tree(total=3, accept=1, managed=0, reject=2):
    """A tree where every surface carries the true numbers."""
    files = {}
    verdicts = ["ACCEPT"] * accept + ["ACCEPT-MANAGED"] * managed + ["REJECT A1c"] * reject
    assert len(verdicts) == total
    for i, v in enumerate(verdicts):
        files[f"tests/checker-corpus/f{i:02d}.hy"] = corpus_file(v)
    files["compiler/crates/hy-syntax/tests/parse_all.rs"] = (
        f'    assert_eq!(files.len(), {total}, "expected exactly {total} corpus programs",);\n'
        '    assert_eq!(files.len(), 10, "expected exactly 10 spike-6 programs");\n'
    )
    files[".github/workflows/ci.yml"] = f"      - name: x (verdict agreement ({total}/{total} + tripwire)\n"
    files["Makefile"] = f'\t@echo "make corpus           — Tier-0 corpus verdicts ({total}/{total})"\n'
    files["README.md"] = f"| x | y | z [checker corpus {total}/{total}](tests/checker-corpus/) |\n"
    files["STATUS.md"] = f"- Phase: something — corpus {total}/{total}, differential FULL\n"
    files["GUARANTEES.md"] = (
        f"  {total} programs, each with an `EXPECTED-VERDICT` header. Programs 01–18 were\n"
    )
    files["compiler/crates/hy-syntax/src/ast.rs"] = (
        f"//! Scope: the surface exercised by tests/checker-corpus/*.hy ({total} programs),\n"
    )
    files["tests/checker-corpus/README.md"] = (
        f"**Current registered corpus: {total} programs** — ACCEPT {accept} · "
        f"ACCEPT-MANAGED {managed} ·\nREJECT {reject} — each with a header.\n"
    )
    return files


class _ProblemsMixin:
    """Shared helper: the problems reported for a tree with one surface altered."""

    def _problems_with(self, rel, content):
        files = consistent_tree()
        files[rel] = content
        tmp, root = make_tree(files)
        self.addCleanup(tmp.cleanup)
        problems, _, _ = MOD.collect_problems(root)
        return [p for p in problems if p.startswith(rel)]


class ConsistentTreeTests(unittest.TestCase):
    def test_every_surface_agreeing_reports_nothing(self):
        tmp, root = make_tree(consistent_tree())
        self.addCleanup(tmp.cleanup)
        problems, total, counts = MOD.collect_problems(root)
        self.assertEqual(problems, [])
        self.assertEqual(total, 3)
        self.assertEqual(counts["REJECT"], 2)

    def test_census_is_derived_from_the_files(self):
        tmp, root = make_tree(consistent_tree(total=4, accept=2, managed=1, reject=1))
        self.addCleanup(tmp.cleanup)
        problems, total, counts = MOD.collect_problems(root)
        self.assertEqual(problems, [])
        self.assertEqual((total, counts["ACCEPT"], counts["ACCEPT-MANAGED"]), (4, 2, 1))


class DriftTests(_ProblemsMixin, unittest.TestCase):
    def test_stale_makefile_is_named(self):
        hits = self._problems_with(
            "Makefile",
            '\t@echo "make corpus           — Tier-0 corpus verdicts (99/99)"\n',
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("says 99", hits[0])

    def test_stale_readme_claim_is_named(self):
        hits = self._problems_with("README.md", "| x | y | z [checker corpus 99/99](p) |\n")
        self.assertEqual(len(hits), 1)
        self.assertIn("says 99", hits[0])

    def test_stale_guarantees_line_is_named(self):
        hits = self._problems_with("GUARANTEES.md", "  99 programs, each with an `EXPECTED-VERDICT` header.\n")
        self.assertEqual(len(hits), 1)
        self.assertIn("says 99", hits[0])

    def test_stale_ast_scope_note_is_named(self):
        hits = self._problems_with(
            "compiler/crates/hy-syntax/src/ast.rs",
            "//! Scope: the surface exercised by tests/checker-corpus/*.hy (99 programs),\n",
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("says 99", hits[0])

    def test_stale_ci_label_is_named(self):
        hits = self._problems_with(
            ".github/workflows/ci.yml", "      - name: x (verdict agreement (99/99 + tripwire)\n"
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("says 99", hits[0])

    def test_spike6_assertion_is_not_mistaken_for_the_corpus(self):
        """parse_all.rs asserts the spike-6 count with the same shape."""
        hits = self._problems_with(
            "compiler/crates/hy-syntax/tests/parse_all.rs",
            '    assert_eq!(files.len(), 3, "expected exactly 3 corpus programs",);\n'
            '    assert_eq!(files.len(), 10, "expected exactly 10 spike-6 programs");\n',
        )
        self.assertEqual(hits, [])

    def test_stale_breakdown_is_named(self):
        hits = self._problems_with(
            "tests/checker-corpus/README.md",
            "**Current registered corpus: 3 programs** — ACCEPT 9 · ACCEPT-MANAGED 0 ·\nREJECT 2 — x.\n",
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("breakdown says ACCEPT 9", hits[0])

    def test_stale_readme_total_is_named(self):
        hits = self._problems_with(
            "tests/checker-corpus/README.md",
            "**Current registered corpus: 7 programs** — ACCEPT 1 · ACCEPT-MANAGED 0 ·\nREJECT 2 — x.\n",
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("says 7 programs", hits[0])


class RatioDenominatorTests(_ProblemsMixin, unittest.TestCase):
    def test_ci_denominator_only_drift_is_named(self):
        hits = self._problems_with(
            ".github/workflows/ci.yml",
            "      - name: x (verdict agreement (3/99 + tripwire)\n",
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("3/99", hits[0])

    def test_makefile_denominator_only_drift_is_named(self):
        hits = self._problems_with(
            "Makefile", '\t@echo "Tier-0 corpus verdicts (3/99)"\n'
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("3/99", hits[0])

    def test_readme_denominator_only_drift_is_named(self):
        hits = self._problems_with("README.md", "checker corpus 3/99\n")
        self.assertEqual(len(hits), 1)
        self.assertIn("3/99", hits[0])

    def test_later_stale_ratio_is_not_hidden_by_a_consistent_one(self):
        hits = self._problems_with(
            "README.md", "checker corpus 3/3\nchecker corpus 3/99\n"
        )
        self.assertEqual(len(hits), 1)
        self.assertIn("3/99", hits[0])

    def test_cli_distinguishes_clean_and_denominator_only_drift(self):
        for denominator, expected_exit in ((3, 0), (99, 1)):
            with self.subTest(denominator=denominator):
                files = consistent_tree()
                files["README.md"] = f"checker corpus 3/{denominator}\n"
                files["tools/check_corpus_census.py"] = SCRIPT.read_text(encoding="utf-8")
                tmp, root = make_tree(files)
                self.addCleanup(tmp.cleanup)
                result = subprocess.run(
                    [sys.executable, str(root / "tools/check_corpus_census.py")],
                    cwd=root, capture_output=True, text=True, timeout=10,
                )
                self.assertEqual(result.returncode, expected_exit,
                                 result.stdout + result.stderr)
                if expected_exit:
                    self.assertIn("README.md", result.stderr)
                    self.assertIn("3/99", result.stderr)
                else:
                    self.assertIn("OK:", result.stdout)


class FailClosedTests(_ProblemsMixin, unittest.TestCase):
    def test_missing_corpus_directory_is_an_error(self):
        tmp, root = make_tree({"README.md": "x\n"})
        self.addCleanup(tmp.cleanup)
        problems, total, _ = MOD.collect_problems(root)
        self.assertEqual(total, 0)
        self.assertTrue(problems)
        self.assertIn("no corpus directory", problems[0])

    def test_empty_corpus_refuses_to_certify(self):
        tmp, root = make_tree({"tests/checker-corpus/.keep": ""})
        self.addCleanup(tmp.cleanup)
        problems, total, _ = MOD.collect_problems(root)
        self.assertEqual(total, 0)
        self.assertIn("empty", problems[0])

    def test_file_without_a_verdict_line_is_reported(self):
        files = consistent_tree()
        files["tests/checker-corpus/broken.hy"] = "fn main() {\n}\n"
        tmp, root = make_tree(files)
        self.addCleanup(tmp.cleanup)
        problems, _, _ = MOD.collect_problems(root)
        self.assertTrue(any("no EXPECTED-VERDICT" in p for p in problems))

    def test_a_surface_losing_its_pattern_is_reported_not_ignored(self):
        """A silently non-matching pattern must fail, not pass vacuously."""
        hits = self._problems_with("Makefile", "# the corpus line moved away\n")
        self.assertEqual(len(hits), 1)
        self.assertIn("no corpus count found", hits[0])


if __name__ == "__main__":
    unittest.main()
