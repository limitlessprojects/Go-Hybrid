#!/usr/bin/env python3
"""Regression coverage for tools/check_conflict_markers.py."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
SCRIPT = TOOLS_DIR / "check_conflict_markers.py"

SPEC = importlib.util.spec_from_file_location("check_conflict_markers", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"unable to load {SCRIPT}")
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


class MarkerDetectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temporary_directories: list[tempfile.TemporaryDirectory[str]] = []

    def tearDown(self) -> None:
        for directory in self._temporary_directories:
            directory.cleanup()

    def _tree(self, files: dict[str, str]) -> Path:
        directory = tempfile.TemporaryDirectory()
        self._temporary_directories.append(directory)
        root = Path(directory.name)

        for relative_name, contents in files.items():
            path = root / relative_name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(contents, encoding="utf-8")

        return root

    def _run(self, root: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(root)],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )

    def test_detects_all_standard_marker_lines(self) -> None:
        root = self._tree({
            "README.md": (
                "<<<<<<< HEAD\n"
                "left\n"
                "=======\n"
                "right\n"
                ">>>>>>> main\n"
            )
        })

        hits = list(MOD.scan(str(root)))

        self.assertEqual([line for _, line, _ in hits], [1, 3, 5])

    def test_detects_markers_only_at_column_zero(self) -> None:
        root = self._tree({
            "src/example.go": (
                "    <<<<<<< indented\n"
                "// ======= documentation\n"
                "<<<<<<< HEAD\n"
                "=======\n"
                ">>>>>>> main\n"
            )
        })

        hits = list(MOD.scan(str(root)))

        self.assertEqual([line for _, line, _ in hits], [3, 4, 5])

    def test_cli_returns_nonzero_for_markers(self) -> None:
        root = self._tree({"src/example.py": "<<<<<<< HEAD\n>>>>>>> main\n"})

        result = self._run(root)

        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("src", result.stdout + result.stderr)
        self.assertIn("conflict marker", result.stdout + result.stderr)

    def test_cli_returns_zero_for_clean_tree(self) -> None:
        root = self._tree({"README.md": "# clean\n"})

        result = self._run(root)

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("conflict markers: none", result.stdout + result.stderr)

    def test_runtime_assembly_markers_fail_the_cli(self) -> None:
        for suffix in ("s", "S"):
            with self.subTest(suffix=suffix):
                root = self._tree({
                    "runtime/go/hybridge/call_amd64." + suffix: (
                        "<<<<<<< HEAD\n"
                        "TEXT old(SB),$0\n"
                        "=======\n"
                        "TEXT new(SB),$0\n"
                        ">>>>>>> main\n"
                    )
                })

                result = self._run(root)

                self.assertEqual(
                    result.returncode,
                    1,
                    result.stdout + result.stderr,
                )
                self.assertIn(
                    "call_amd64." + suffix,
                    result.stdout + result.stderr,
                )

    def test_clean_assembly_and_binary_file_remain_clean(self) -> None:
        root = self._tree({
            "runtime/go/hybridge/call_arm64.s": (
                "// Go assembly\n"
                "TEXT call(SB),$0\n"
            )
        })
        (root / "blob.bin").write_bytes(
            b"\xff<<<<<<< HEAD\n>>>>>>> main\n"
        )

        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_unknown_extension_is_skipped(self) -> None:
        root = self._tree({
            "weird.xyz": "<<<<<<< HEAD\n>>>>>>> main\n"
        })

        self.assertEqual(list(MOD.scan(str(root))), [])

    def test_skips_known_generated_and_dependency_directories(self) -> None:
        root = self._tree({
            ".git/index": "<<<<<<< HEAD\n>>>>>>> main\n",
            "target/output.rs": "<<<<<<< HEAD\n>>>>>>> main\n",
            "node_modules/pkg/index.js": "<<<<<<< HEAD\n>>>>>>> main\n",
            ".venv/lib/site.py": "<<<<<<< HEAD\n>>>>>>> main\n",
            "__pycache__/cache.py": "<<<<<<< HEAD\n>>>>>>> main\n",
        })

        self.assertEqual(list(MOD.scan(str(root))), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
