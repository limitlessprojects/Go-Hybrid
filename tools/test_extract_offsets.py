"""Regression tests for tools/extract_offsets.py DWARF offset extraction."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
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

INDIRECT_STRP_AND_UCONST_DWARF_OUTPUT = """
 <1><50>: Abbrev Number: 9 (DW_TAG_structure_type)
    DW_AT_name        : (indirect string, offset: 0x10): runtime.g
    DW_AT_declaration : 1
 <2><58>: Abbrev Number: 0
 <1><100>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : (indirect string, offset: 0x10): runtime.g
    DW_AT_byte_size   : 376
 <2><110>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : (indirect string, offset: 0x20): stack
    DW_AT_data_member_location: 2 byte block: 23 0 \t(DW_OP_plus_uconst: 0)
 <2><120>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : (indirect string, offset: 0x30): stackguard0
    DW_AT_data_member_location: 2 byte block: 23 10 \t(DW_OP_plus_uconst: 16)
 <2><130>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : (indirect string, offset: 0x40): m
    DW_AT_data_member_location: 2 byte block: 23 30 \t(DW_OP_plus_uconst: 48)
 <2><140>: Abbrev Number: 0
 <1><200>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : (indexed string: 0x05): runtime.m
    DW_AT_byte_size   : 504
 <2><210>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : g0
    DW_AT_data_member_location: 2 byte block: 23 0 \t(DW_OP_plus_uconst: 0)
 <2><220>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : curg
    DW_AT_data_member_location: 2 byte block: 23 18 \t(DW_OP_plus_uconst: 24)
 <2><230>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : id
    DW_AT_data_member_location: 2 byte block: 23 68 \t(DW_OP_plus_uconst: 104)
 <2><240>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : locks
    DW_AT_data_member_location: 2 byte block: 23 70 \t(DW_OP_plus_uconst: 112)
"""


class ExtractOffsetsTests(unittest.TestCase):
    def test_readelf_non_zero_fails(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "bad_binary"],
            returncode=1,
            stdout="",
            stderr="readelf: Error: 'bad_binary': No such file",
        )
        stderr_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stderr(stderr_buf):
                with self.assertRaises(SystemExit) as cm:
                    extractor.main("bad_binary")
        self.assertNotEqual(cm.exception.code, 0)
        err_msg = stderr_buf.getvalue()
        self.assertIn("1", err_msg)
        self.assertIn("readelf: Error: 'bad_binary': No such file", err_msg)

    def test_missing_required_member_fails(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=MISSING_MEMBER_DWARF_OUTPUT,
            stderr="",
        )
        stderr_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stderr(stderr_buf):
                with self.assertRaises(SystemExit) as cm:
                    extractor.main("test_binary")
        self.assertNotEqual(cm.exception.code, 0)
        err_msg = stderr_buf.getvalue()
        self.assertIn("stack", err_msg)

    def test_valid_dwarf_fixture_succeeds(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=VALID_DWARF_OUTPUT,
            stderr="",
        )
        stdout_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stdout(stdout_buf):
                extractor.main("test_binary")
        output = stdout_buf.getvalue()
        self.assertIn("# runtime.g size=376  runtime.m size=504  (from test_binary)", output)
        self.assertIn("g_m              = 48", output)
        self.assertIn("g_stackguard0    = 16", output)
        self.assertIn("g_stack_lo       = 0", output)
        self.assertIn("g_stack_hi       = 8", output)
        self.assertIn("m_g0             = 0", output)
        self.assertIn("m_curg           = 24", output)
        self.assertIn("m_id             = 104", output)
        self.assertIn("m locks          = 112", output)

    def test_indirect_strings_uconst_blocks_and_forward_declarations_succeed(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=INDIRECT_STRP_AND_UCONST_DWARF_OUTPUT,
            stderr="",
        )
        stdout_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stdout(stdout_buf):
                extractor.main("test_binary")
        output = stdout_buf.getvalue()
        self.assertIn("# runtime.g size=376  runtime.m size=504  (from test_binary)", output)
        self.assertIn("g_m              = 48", output)
        self.assertIn("g_stackguard0    = 16", output)
        self.assertIn("g_stack_lo       = 0", output)
        self.assertIn("g_stack_hi       = 8", output)
        self.assertIn("m_curg           = 24", output)
        self.assertIn("m_id             = 104", output)
        self.assertIn("m locks          = 112", output)


if __name__ == "__main__":
    unittest.main()
