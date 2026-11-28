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


MARKER_BLOCK = "<<<<<<< HEAD\nold\n=======\nnew\n>>>>>>> main\n"


class GitAwareScanTests(unittest.TestCase):
    """candidate_paths() must follow Git's own view of a checkout: tracked
    files regardless of .gitignore, untracked files unless ignored."""

    def _repo(self):
        tmp = tempfile.TemporaryDirectory(prefix="gitmarkers-")
        root = Path(tmp.name)
        self.addCleanup(tmp.cleanup)
        self._git(root, "init", "-q")
        (root / ".gitignore").write_text("results/\n/build/\n", encoding="utf-8")
        (root / "README.md").write_text("clean\n", encoding="utf-8")
        self._git(root, "add", ".gitignore", "README.md")
        self._git(root, "-c", "user.name=Fixture", "-c",
                   "user.email=fixture@example.invalid", "commit", "-qm", "fixture")
        return root

    def _git(self, root, *args):
        subprocess.run(["git", *args], cwd=str(root), check=True,
                        capture_output=True, timeout=20)

    def _write(self, root, rel, content):
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")
        return p

    def test_ignored_untracked_marker_is_not_reported(self):
        root = self._repo()
        self._write(root, "results/old.py", MARKER_BLOCK)
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_force_added_ignored_file_is_reported(self):
        root = self._repo()
        self._write(root, "results/old.py", MARKER_BLOCK)
        self._git(root, "add", "-f", "results/old.py")
        hits = list(MOD.scan(str(root)))
        self.assertEqual([h[0] for h in hits], ["results/old.py"])

    def test_tracked_file_in_skipped_dir_is_reported(self):
        # target/ is in SKIP_DIRS for the non-Git walk, but a force-added
        # tracked file there must still be caught in a real checkout.
        root = self._repo()
        self._write(root, "target/tracked.py", MARKER_BLOCK)
        self._git(root, "add", "-f", "target/tracked.py")
        hits = list(MOD.scan(str(root)))
        self.assertEqual([h[0] for h in hits], [os.path.join("target", "tracked.py")])

    def test_new_unignored_file_with_spaces_is_reported(self):
        root = self._repo()
        self._write(root, "new file.md", MARKER_BLOCK)
        hits = list(MOD.scan(str(root)))
        self.assertEqual([h[0] for h in hits], ["new file.md"])

    def test_tracked_modification_is_reported(self):
        root = self._repo()
        self._write(root, "README.md", "clean v2\n")
        self._git(root, "add", "README.md")
        self._git(root, "-c", "user.name=Fixture", "-c",
                   "user.email=fixture@example.invalid", "commit", "-qm", "second")
        self._write(root, "README.md", MARKER_BLOCK)
        hits = list(MOD.scan(str(root)))
        self.assertEqual([h[0] for h in hits], ["README.md"])

    def test_deleted_tracked_file_is_not_read(self):
        root = self._repo()
        self._write(root, "doomed.md", MARKER_BLOCK)
        self._git(root, "add", "doomed.md")
        os.remove(root / "doomed.md")
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_symlink_to_external_content_is_not_read(self):
        root = self._repo()
        outside = Path(tempfile.mkdtemp(prefix="outside-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(outside, ignore_errors=True))
        target = outside / "evil.md"
        target.write_text(MARKER_BLOCK, encoding="utf-8")
        link = root / "link.md"
        try:
            os.symlink(target, link)
        except (OSError, NotImplementedError):
            self.skipTest("symlinks unsupported on this platform")
        self._git(root, "add", "link.md")
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_linked_worktree_uses_git_rules(self):
        root = self._repo()
        wt = Path(tempfile.mkdtemp(prefix="gitmarkers-wt-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(wt, ignore_errors=True))
        subprocess.run(
            ["git", "worktree", "add", "-q", str(wt), "-b", "wt-branch"],
            cwd=str(root), check=True, capture_output=True, timeout=20)
        self.assertTrue(os.path.isfile(wt / ".git"))  # a file, not a directory
        self._write(wt, "results/old.py", MARKER_BLOCK)  # ignored, untracked
        self._write(wt, "tracked.py", MARKER_BLOCK)
        self._git(wt, "add", "tracked.py")
        hits = list(MOD.scan(str(wt)))
        self.assertEqual([h[0] for h in hits], ["tracked.py"])

    def test_non_checkout_tree_keeps_the_old_walk(self):
        # No .git: behaves like an exported archive, SKIP_DIRS still applies.
        root = self._repo()
        self._write(root, "target/tracked.py", MARKER_BLOCK)
        import shutil
        shutil.rmtree(root / ".git")
        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_mocked_git_launch_failure_falls_back_to_walk(self):
        root = self._repo()
        self._write(root, "results/old.py", MARKER_BLOCK)

        def _raise(*a, **k):
            raise OSError("git not found")

        orig = MOD.subprocess.run
        MOD.subprocess.run = _raise
        try:
            hits = list(MOD.scan(str(root)))
        finally:
            MOD.subprocess.run = orig
        # Falls back to the plain walk, which has no ignore awareness.
        self.assertEqual([h[0] for h in hits], ["results/old.py"])


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
