"""Tests for tools/check_diagnostic_registry.py (stdlib only, no network).

The guard exists because the compiler could print a code the registry did not
carry and nothing failed: `HYB-E4004` and `HYB-E4005` shipped that way, so
`hyb explain` answered "unknown diagnostic code" for a real check failure
(issue #4460). These tests pin both directions of the join — a clean tree
passes, an emitted-but-unregistered code fails with its `file:line` — and pin
that a comment mention counts, because a doc comment naming a code that does
not exist is the same drift one step earlier.
"""
import contextlib
import importlib.util
import io
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check_diagnostic_registry.py")
SPEC = importlib.util.spec_from_file_location("diagnostic_registry", SCRIPT)
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)

REGISTRY = "compiler/crates/hy-diag/data/diagnostics.json"


def entry(code):
    return {
        "code": code,
        "name": code.lower().replace("-", " "),
        "category": "type",
        "severity": "error",
        "title": "t",
        "summary": "s",
        "fixits": ["f"],
        "example": "e",
        "rule": "",
        "tier0_rule_id": None,
    }


def make_tree(files):
    tmp = tempfile.TemporaryDirectory(prefix="registry-")
    root = Path(tmp.name)
    for rel, content in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    return tmp, root


def tree_with(registry_codes, rust_files):
    import json

    files = {
        REGISTRY: json.dumps(
            {
                "version": 1,
                "updated": "2026-11-27",
                "format": "test",
                "categories": {"E4": "test"},
                "codes": [entry(c) for c in registry_codes],
            }
        )
    }
    files.update(rust_files)
    return make_tree(files)


def run(root):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = MOD.main(["--root", str(root)])
    return code, out.getvalue(), err.getvalue()


class CleanCase(unittest.TestCase):
    def test_registered_code_passes(self):
        tmp, root = tree_with(
            ["HYB-E3001"],
            {"compiler/crates/hy-check/src/a.rs": 'diag("HYB-E3001");\n'},
        )
        self.addCleanup(tmp.cleanup)
        code, out, err = run(root)
        self.assertEqual(code, 0, err)
        self.assertIn("OK", out)

    def test_tree_with_no_mentions_passes(self):
        tmp, root = tree_with(
            ["HYB-E3001"], {"compiler/crates/hy-check/src/a.rs": "fn main() {}\n"}
        )
        self.addCleanup(tmp.cleanup)
        code, _, err = run(root)
        self.assertEqual(code, 0, err)


class DriftCase(unittest.TestCase):
    def test_emitted_but_unregistered_fails_with_site(self):
        tmp, root = tree_with(
            ["HYB-E3001"],
            {
                "compiler/crates/hy-check/src/channel_liveness.rs": (
                    "fn a() {}\nfn b() { diag(\"HYB-E4004\"); }\n"
                )
            },
        )
        self.addCleanup(tmp.cleanup)
        code, _, err = run(root)
        self.assertEqual(code, 1)
        self.assertIn("HYB-E4004", err)
        self.assertIn("channel_liveness.rs:2", err)

    def test_doc_comment_mention_counts(self):
        # A code named only in a doc comment is the same drift one step earlier.
        tmp, root = tree_with(
            ["HYB-E3001"],
            {"compiler/crates/hy-check/src/a.rs": "//! emits HYB-E4005 at scope exit\n"},
        )
        self.addCleanup(tmp.cleanup)
        code, _, err = run(root)
        self.assertEqual(code, 1)
        self.assertIn("HYB-E4005", err)

    def test_every_unregistered_code_is_named(self):
        tmp, root = tree_with(
            [],
            {
                "compiler/crates/a/src/x.rs": 'diag("HYB-E4004");\n',
                "compiler/crates/b/src/y.rs": 'diag("HYB-E4005");\n',
            },
        )
        self.addCleanup(tmp.cleanup)
        code, _, err = run(root)
        self.assertEqual(code, 1)
        self.assertIn("2 code(s)", err)
        self.assertIn("HYB-E4004", err)
        self.assertIn("HYB-E4005", err)


class MisuseCase(unittest.TestCase):
    def test_missing_registry_is_misuse(self):
        tmp, root = make_tree(
            {"compiler/crates/hy-check/src/a.rs": 'diag("HYB-E3001");\n'}
        )
        self.addCleanup(tmp.cleanup)
        code, _, err = run(root)
        self.assertEqual(code, 2)
        self.assertIn("no registry", err)

    def test_registry_without_codes_list_is_misuse(self):
        tmp, root = make_tree(
            {
                REGISTRY: '{"version": 1}',
                "compiler/crates/hy-check/src/a.rs": 'diag("HYB-E3001");\n',
            }
        )
        self.addCleanup(tmp.cleanup)
        code, _, err = run(root)
        self.assertEqual(code, 2)
        self.assertIn("codes", err)

    def test_root_without_path_is_misuse(self):
        err = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = MOD.main(["--root"])
        self.assertEqual(code, 2)
        self.assertIn("--root needs a path", err.getvalue())

    def test_unknown_argument_is_misuse(self):
        err = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = MOD.main(["--bogus"])
        self.assertEqual(code, 2)
        self.assertIn("unknown argument", err.getvalue())


class RealRepoCase(unittest.TestCase):
    """The repository this tool lives in must be clean (that is the point)."""

    def test_repo_registry_is_closed(self):
        root = MOD.REPO_ROOT
        if not (root / MOD.REGISTRY_REL).is_file():
            self.skipTest("registry not present in this checkout")
        missing, total = MOD.unregistered(root)
        self.assertEqual(missing, {}, f"unregistered codes: {sorted(missing)}")
        self.assertGreater(total, 0)


if __name__ == "__main__":
    unittest.main()
