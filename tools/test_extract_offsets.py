"""Regression tests for tools/extract_offsets.py DWARF offset extraction."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "extract_offsets", Path(__file__).with_name("extract_offsets.py")
)
extractor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extractor)

VALID_DWARF_OUTPUT = """
 <1><100>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
    DW_AT_byte_size   : 376
 <2><110>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stack
    DW_AT_data_member_location: 0
 <2><120>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stackguard0
    DW_AT_data_member_location: 16
 <2><130>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : m
    DW_AT_data_member_location: 48
 <1><200>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.m
    DW_AT_byte_size   : 504
 <2><210>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : g0
    DW_AT_data_member_location: 0
 <2><220>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : curg
    DW_AT_data_member_location: 24
 <2><230>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : id
    DW_AT_data_member_location: 104
 <2><240>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : locks
    DW_AT_data_member_location: 112
"""

MISSING_MEMBER_DWARF_OUTPUT = """
 <1><100>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
    DW_AT_byte_size   : 376
 <2><110>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stackguard0
    DW_AT_data_member_location: 16
 <2><130>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : m
    DW_AT_data_member_location: 48
 <1><200>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.m
    DW_AT_byte_size   : 504
 <2><210>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : g0
    DW_AT_data_member_location: 0
 <2><220>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : curg
    DW_AT_data_member_location: 24
 <2><230>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : id
    DW_AT_data_member_location: 104
 <2><240>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : locks
    DW_AT_data_member_location: 112
"""


@contextlib.contextmanager
def _temp_executable(name: str):
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / name
        path.write_bytes(b"\x7fELF")
        os.chmod(path, 0o755)
        yield str(path)


class ExtractOffsetsTests(unittest.TestCase):
    def test_missing_or_directory_or_non_exec_binpath_fails_before_readelf(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            missing = str(Path(tmpdir) / "does_not_exist")
            non_exec = Path(tmpdir) / "non_exec_bin"
            non_exec.write_bytes(b"\x7fELF")
            os.chmod(non_exec, 0o644)

            for candidate, expected_substr in (
                ("", "binary path is required"),
                (missing, "does not exist or is not a regular file"),
                (tmpdir, "does not exist or is not a regular file"),
                (str(non_exec), "binary is not executable"),
            ):
                stderr_buf = io.StringIO()
                with patch.object(extractor.subprocess, "run") as run_mock:
                    with contextlib.redirect_stderr(stderr_buf):
                        with self.assertRaises(SystemExit) as cm:
                            extractor.main(candidate)
                    run_mock.assert_not_called()
                self.assertEqual(cm.exception.code, 2)
                self.assertIn(expected_substr, stderr_buf.getvalue())

    def test_readelf_non_zero_fails(self):
        with _temp_executable("bad_binary") as binpath:
            proc = subprocess.CompletedProcess(
                ["readelf", "--debug-dump=info", binpath],
                returncode=1,
                stdout="",
                stderr=f"readelf: Error: '{binpath}': Not an ELF file",
            )
            stderr_buf = io.StringIO()
            with patch.object(extractor.subprocess, "run", return_value=proc):
                with contextlib.redirect_stderr(stderr_buf):
                    with self.assertRaises(SystemExit) as cm:
                        extractor.main(binpath)
            self.assertNotEqual(cm.exception.code, 0)
            err_msg = stderr_buf.getvalue()
            self.assertIn("1", err_msg)
            self.assertIn("Not an ELF file", err_msg)

    def test_missing_required_member_fails(self):
        with _temp_executable("test_binary") as binpath:
            proc = subprocess.CompletedProcess(
                ["readelf", "--debug-dump=info", binpath],
                returncode=0,
                stdout=MISSING_MEMBER_DWARF_OUTPUT,
                stderr="",
            )
            stderr_buf = io.StringIO()
            with patch.object(extractor.subprocess, "run", return_value=proc):
                with contextlib.redirect_stderr(stderr_buf):
                    with self.assertRaises(SystemExit) as cm:
                        extractor.main(binpath)
            self.assertNotEqual(cm.exception.code, 0)
            err_msg = stderr_buf.getvalue()
            self.assertIn("stack", err_msg)

    def test_valid_dwarf_fixture_succeeds(self):
        with _temp_executable("test_binary") as binpath:
            proc = subprocess.CompletedProcess(
                ["readelf", "--debug-dump=info", binpath],
                returncode=0,
                stdout=VALID_DWARF_OUTPUT,
                stderr="",
            )
            stdout_buf = io.StringIO()
            with patch.object(extractor.subprocess, "run", return_value=proc):
                with contextlib.redirect_stdout(stdout_buf):
                    extractor.main(binpath)
            output = stdout_buf.getvalue()
            self.assertIn(f"# runtime.g size=376  runtime.m size=504  (from {binpath})", output)
            self.assertIn("g_m              = 48", output)
            self.assertIn("g_stackguard0    = 16", output)
            self.assertIn("g_stack_lo       = 0", output)
            self.assertIn("g_stack_hi       = 8", output)
            self.assertIn("m_g0             = 0", output)
            self.assertIn("m_curg           = 24", output)
            self.assertIn("m_id             = 104", output)
            self.assertIn("m locks          = 112", output)


if __name__ == "__main__":
    unittest.main()
