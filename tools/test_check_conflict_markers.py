"""Tests for tools/check_conflict_markers.py (stdlib only, no network)."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("check_conflict_markers.py")
SPEC = importlib.util.spec_from_file_location("conflict_markers", SCRIPT)
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


class MarkerDetectionTests(unittest.TestCase):
    def _tree(self, files):
        tmp = tempfile.TemporaryDirectory(prefix="markers-")
        root = Path(tmp.name)
        for rel, content in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content, encoding="utf-8")
        self.addCleanup(tmp.cleanup)
        return root

    def test_clean_tree_reports_nothing(self):
        root = self._tree({
            "README.md": "hello\nworld\n",
            "ci.yml": "on: push\njobs:\n  a:\n    steps: []\n",
        })
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_full_conflict_block_is_found(self):
        root = self._tree({"ci.yml": "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> main\nd\n"})
        hits = list(MOD.scan(str(root)))
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0][0], "ci.yml")
        self.assertEqual(hits[0][1], 2)

    def test_unterminated_start_is_still_reported(self):
        root = self._tree({"x.md": "a\n<<<<<<< HEAD\nb\n"})
        hits = list(MOD.scan(str(root)))
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0][1], 2)

    def test_lone_end_marker_is_not_reported(self):
        root = self._tree({"x.md": "a\n>>>>>>> main\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_setext_underline_is_not_a_marker(self):
        # `=======` under a heading is legitimate Markdown, not a conflict.
        root = self._tree({"doc.md": "Title\n=======\n\ntext\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_indented_marker_is_not_reported(self):
        root = self._tree({"code.py": "    <<<<<<< HEAD\n    >>>>>>> x\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_two_blocks_in_one_file_are_both_found(self):
        root = self._tree({
            "y.md": "<<<<<<< a\n1\n>>>>>>> b\nok\n<<<<<<< c\n2\n>>>>>>> d\n"
        })
        hits = list(MOD.scan(str(root)))
        self.assertEqual(sorted(h[1] for h in hits), [1, 5])

    def test_unknown_extension_is_skipped(self):
        root = self._tree({"weird.xyz": "<<<<<<< HEAD\n>>>>>>> main\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_runtime_assembly_markers_fail_the_cli(self):
        for suffix in ("s", "S"):
            with self.subTest(suffix=suffix):
                root = self._tree({
                    "runtime/go/hybridge/call_amd64." + suffix:
                        "<<<<<<< HEAD\nTEXT old(SB),$0\n=======\nTEXT new(SB),$0\n>>>>>>> main\n"
                })
                result = subprocess.run(
                    [sys.executable, str(SCRIPT), "--root", str(root)],
                    capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn("call_amd64." + suffix, result.stdout + result.stderr)

    def test_clean_assembly_and_binary_file_remain_clean(self):
        root = self._tree({"runtime/go/hybridge/call_arm64.s":
                           "// Go assembly\nTEXT call(SB),$0\n"})
        (root / "blob.bin").write_bytes(b"\xff<<<<<<< HEAD\n>>>>>>> main\n")
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_git_directory_is_skipped(self):
        root = self._tree({".git/COMMIT_EDITMSG": "<<<<<<< HEAD\n>>>>>>> main\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])


class WorkflowStructureTests(unittest.TestCase):
    """The other outage: a column-zero line that cannot parse as a mapping."""

    def _tree(self, files):
        tmp = tempfile.TemporaryDirectory(prefix="wfstruct-")
        root = Path(tmp.name)
        for rel, content in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content, encoding="utf-8")
        self.addCleanup(tmp.cleanup)
        return root

    def test_column_zero_shell_line_is_reported(self):
        # This is the 96f0bc4 shape: an unindented duplicate of a `run: |`
        # line, which ends the block scalar and breaks the mapping.
        root = self._tree({
            ".github/workflows/ci.yml": (
                "name: ci\non:\n  push:\n    branches:\n      - main\n"
                "jobs:\n  quick:\n    steps:\n      - name: t\n"
                "        run: |\n          set -e\n"
                "python3 -m py_compile tools/job.py\n"
                "          python3 -m unittest discover -s tools\n"
            ),
        })
        hits = [h for h in MOD.scan(str(root)) if h[3] == "workflow"]
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0][1], 12)

    def test_well_formed_workflow_is_clean(self):
        root = self._tree({
            ".github/workflows/ci.yml": (
                "# a comment\nname: ci\non:\n  push:\njobs:\n  quick:\n"
                "    steps:\n      - name: t\n        run: |\n"
                "          set -e\n          echo hi\n"
            ),
        })
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_document_marker_is_allowed(self):
        root = self._tree({".github/workflows/ci.yml": "---\nname: ci\non: push\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_column_zero_key_is_allowed(self):
        root = self._tree({
            ".github/workflows/ci.yml": "name: ci\non: push\npermissions:\n  contents: read\njobs: {}\n",
        })
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_non_workflow_yaml_is_not_checked_for_structure(self):
        # Only `.github/workflows/*.yml` is a GitHub Actions mapping; a loose
        # YAML file elsewhere may legitimately wrap lines at column zero.
        root = self._tree({"jobs/001/x.yml": "python3 -m py_compile tools/job.py\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_fenced_block_in_markdown_is_not_checked(self):
        root = self._tree({"doc.md": "```\npython3 -m py_compile tools/job.py\n```\n"})
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_yaml_extension_workflow_is_checked(self):
        # GitHub Actions also supports .github/workflows/*.yaml. A column-zero
        # defect in a .yaml file must be caught just like a .yml file.
        root = self._tree({
            ".github/workflows/deploy.yaml": (
                "name: deploy\non:\n  push:\njobs:\n  deploy:\n    steps:\n"
                "      - run: |\n          set -e\n"
                "echo unindented shell line\n"
            ),
        })
        hits = [h for h in MOD.scan(str(root)) if h[3] == "workflow"]
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0][0], os.path.join(".github", "workflows", "deploy.yaml"))
        self.assertEqual(hits[0][1], 9)

    def test_document_end_marker_is_allowed(self):
        # A valid YAML document terminating with '...' at column zero.
        root = self._tree({
            ".github/workflows/ci.yml": "---\nname: ci\non: push\njobs: {}\n...\n",
        })
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_quoted_top_level_keys_are_allowed(self):
        # Quoted keys such as 'on': or "name": are standard in Actions YAML
        # to avoid boolean coercion under YAML 1.1 parsers.
        root = self._tree({
            ".github/workflows/ci.yml": "'on':\n  push:\n\"name\": ci\njobs: {}\n",
        })
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_yaml_directive_is_allowed(self):
        # Directives such as %YAML 1.2 are valid column-zero lines in YAML.
        root = self._tree({
            ".github/workflows/ci.yml": "%YAML 1.2\n---\nname: ci\non: push\njobs: {}\n",
        })
        self.assertEqual(list(MOD.scan(str(root))), [])


class CliTests(unittest.TestCase):
    def _run(self, root):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(root)],
            capture_output=True, text=True, timeout=20,
        )

    def test_clean_exits_zero(self):
        with tempfile.TemporaryDirectory() as td:
            Path(td, "a.md").write_text("ok\n", encoding="utf-8")
            res = self._run(td)
            self.assertEqual(res.returncode, 0, res.stderr)
            self.assertIn("none", res.stdout)

    def test_marker_exits_one_and_names_the_file(self):
        with tempfile.TemporaryDirectory() as td:
            Path(td, "broken.yml").write_text(
                "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> main\n", encoding="utf-8")
            res = self._run(td)
            self.assertEqual(res.returncode, 1)
            self.assertIn("broken.yml", res.stderr)
            self.assertIn("conflict marker", res.stderr.lower())

    def test_missing_root_is_usage_error(self):
        res = subprocess.run(
            [sys.executable, str(SCRIPT), "--root", "no-such-dir-xyz"],
            capture_output=True, text=True, timeout=20,
        )
        self.assertEqual(res.returncode, 2)

    def test_broken_workflow_exits_one_and_names_the_file(self):
        with tempfile.TemporaryDirectory() as td:
            d = Path(td, ".github", "workflows")
            d.mkdir(parents=True)
            Path(d, "ci.yml").write_text(
                "name: ci\non: push\njobs:\n  quick:\n    steps:\n"
                "      - name: t\n        run: |\n          set -e\n"
                "python3 -m py_compile tools/job.py\n",
                encoding="utf-8")
            res = self._run(td)
            self.assertEqual(res.returncode, 1, res.stderr)
            self.assertIn("ci.yml", res.stderr)
            self.assertIn("column zero", res.stderr)

    def test_broken_yaml_workflow_exits_one_and_names_the_file(self):
        with tempfile.TemporaryDirectory() as td:
            d = Path(td, ".github", "workflows")
            d.mkdir(parents=True)
            Path(d, "release.yaml").write_text(
                "name: release\non: push\njobs:\n  quick:\n    steps:\n"
                "      - name: t\n        run: |\n          set -e\n"
                "python3 -m py_compile tools/job.py\n",
                encoding="utf-8")
            res = self._run(td)
            self.assertEqual(res.returncode, 1, res.stderr)
            self.assertIn("release.yaml", res.stderr)
            self.assertIn("column zero", res.stderr)

    def test_marker_takes_priority_in_the_same_run(self):
        with tempfile.TemporaryDirectory() as td:
            Path(td, "broken.yml").write_text(
                "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> main\n", encoding="utf-8")
            res = self._run(td)
            self.assertEqual(res.returncode, 1)
            self.assertIn("conflict marker", res.stderr.lower())


if __name__ == "__main__":
    unittest.main()
