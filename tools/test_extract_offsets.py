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

    def test_forward_declarations_and_empty_stubs_are_skipped_for_full_definitions(self):
        dwarf = """
 <1><050>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
    DW_AT_declaration : 1
 <1><060>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.m
 <2><068>: Abbrev Number: 0
""" + VALID_DWARF_OUTPUT
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=dwarf,
            stderr="",
        )
        stdout_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stdout(stdout_buf):
                extractor.main("test_binary")
        output = stdout_buf.getvalue()
        self.assertIn("# runtime.g size=376  runtime.m size=504  (from test_binary)", output)
        self.assertIn("g_stackguard0    = 16", output)
        self.assertIn("g_m              = 48", output)
        self.assertIn("m_curg           = 24", output)

    def test_nested_struct_members_and_null_dies_do_not_overwrite_direct_fields(self):
        dwarf = """
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
 <2><135>: Abbrev Number: 3 (DW_TAG_structure_type)
    DW_AT_name        : nested_inner
 <3><138>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : m
    DW_AT_data_member_location: 999
 <3><13a>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stack
    DW_AT_data_member_location: 888
 <3><13c>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stackguard0
    DW_AT_data_member_location: 777
 <3><13f>: Abbrev Number: 0
    DW_AT_data_member_location: 666
 <2><140>: Abbrev Number: 0
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
 <2><250>: Abbrev Number: 3 (DW_TAG_structure_type)
    DW_AT_name        : nested_m_inner
 <3><255>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : locks
    DW_AT_data_member_location: 555
 <3><258>: Abbrev Number: 0
 <2><260>: Abbrev Number: 0
"""
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=dwarf,
            stderr="",
        )
        stdout_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stdout(stdout_buf):
                extractor.main("test_binary")
        output = stdout_buf.getvalue()
        self.assertIn("g_m              = 48", output)
        self.assertIn("g_stackguard0    = 16", output)
        self.assertIn("g_stack_lo       = 0", output)
        self.assertIn("g_stack_hi       = 8", output)
        self.assertIn("m locks          = 112", output)

    def test_indirect_strings_hex_sizes_and_dw_op_plus_uconst_locations_succeed(self):
        dwarf = """
 <1><1A0>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : (indirect string, offset: 0x12a4): runtime.g
    DW_AT_byte_size   : 0x178
 <2><1B0>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : (indirect string, offset: 0x12b0): stack
    DW_AT_data_member_location: 2 byte block: 23 0 	(DW_OP_plus_uconst: 0)
 <2><1C0>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : (indirect string, offset: 0x12c0): stackguard0
    DW_AT_data_member_location: 2 byte block: 23 10 	(DW_OP_plus_uconst: 16)
 <2><1D0>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : (indexed string: 0x04): m
    DW_AT_data_member_location: 0x30
 <1><2A0>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : (indirect string, offset: 0x2000): runtime.m
    DW_AT_byte_size   : 0x1f8
 <2><2B0>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : g0
    DW_AT_data_member_location: (DW_OP_plus_uconst: 0)
 <2><2C0>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : curg
    DW_AT_data_member_location: 2 byte block: 23 18 (DW_OP_plus_uconst: 24)
 <2><2D0>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : id
    DW_AT_data_member_location: (DW_OP_plus_uconst: 0x68)
 <2><2E0>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : locks
    DW_AT_data_member_location: 0x70
"""
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=dwarf,
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

    def test_readelf_oserror_fails_cleanly(self):
        stderr_buf = io.StringIO()
        with patch.object(
            extractor.subprocess, "run", side_effect=FileNotFoundError("readelf not found")
        ):
            with contextlib.redirect_stderr(stderr_buf):
                with self.assertRaises(SystemExit) as cm:
                    extractor.main("test_binary")
        self.assertEqual(cm.exception.code, 1)
        self.assertIn("FAILED:", stderr_buf.getvalue())
        self.assertIn("readelf not found", stderr_buf.getvalue())


if __name__ == "__main__":
    unittest.main()
