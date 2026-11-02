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

HEX_AND_EXPR_DWARF_OUTPUT = """
 <1><100>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
    DW_AT_byte_size   : 0x178
 <2><110>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stack
    DW_AT_data_member_location: 0x0
 <2><120>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stackguard0
    DW_AT_data_member_location: 0x10
 <2><130>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : m
    DW_AT_data_member_location: 2 byte block: 23 30 \t(DW_OP_plus_uconst: 48)
 <1><200>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.m
    DW_AT_byte_size   : 0x1f8
 <2><210>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : g0
    DW_AT_data_member_location: 0
 <2><220>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : curg
    DW_AT_data_member_location: 0x18
 <2><230>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : id
    DW_AT_data_member_location: 0x68
 <2><240>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : locks
    DW_AT_data_member_location: (DW_OP_plus_uconst: 112)
"""

FORWARD_DECL_DWARF_OUTPUT = """
 <1><50>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
    DW_AT_declaration : 1
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

NESTED_STRUCT_DWARF_OUTPUT = """
 <1><100>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
    DW_AT_byte_size   : 376
 <2><110>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stack
    DW_AT_data_member_location: 0
 <2><115>: Abbrev Number: 3 (DW_TAG_structure_type)
    DW_AT_name        : stack_inner
 <3><116>: Abbrev Number: 4 (DW_TAG_member)
    DW_AT_name        : stackguard0
    DW_AT_data_member_location: 9999
 <3><117>: Abbrev Number: 4 (DW_TAG_member)
    DW_AT_name        : m
    DW_AT_data_member_location: 8888
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

MISSING_SIZE_DWARF_OUTPUT = """
 <1><100>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
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

MALFORMED_OFFSET_DWARF_OUTPUT = """
 <1><100>: Abbrev Number: 1 (DW_TAG_structure_type)
    DW_AT_name        : runtime.g
    DW_AT_byte_size   : 376
 <2><110>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stack
    DW_AT_data_member_location: 0
 <2><120>: Abbrev Number: 2 (DW_TAG_member)
    DW_AT_name        : stackguard0
    DW_AT_data_member_location: 16_corrupt
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

    def test_hex_and_expression_offsets_succeed(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=HEX_AND_EXPR_DWARF_OUTPUT,
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
        self.assertIn("m_curg           = 24", output)
        self.assertIn("m_id             = 104", output)
        self.assertIn("m locks          = 112", output)

    def test_forward_declaration_is_skipped(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=FORWARD_DECL_DWARF_OUTPUT,
            stderr="",
        )
        stdout_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stdout(stdout_buf):
                extractor.main("test_binary")
        output = stdout_buf.getvalue()
        self.assertIn("# runtime.g size=376  runtime.m size=504  (from test_binary)", output)
        self.assertIn("g_stackguard0    = 16", output)

    def test_nested_struct_members_do_not_clobber_outer(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=NESTED_STRUCT_DWARF_OUTPUT,
            stderr="",
        )
        stdout_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stdout(stdout_buf):
                extractor.main("test_binary")
        output = stdout_buf.getvalue()
        self.assertIn("g_stackguard0    = 16", output)
        self.assertIn("g_m              = 48", output)
        self.assertNotIn("9999", output)
        self.assertNotIn("8888", output)

    def test_missing_byte_size_fails(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=MISSING_SIZE_DWARF_OUTPUT,
            stderr="",
        )
        stderr_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stderr(stderr_buf):
                with self.assertRaises(SystemExit) as cm:
                    extractor.main("test_binary")
        self.assertEqual(cm.exception.code, 1)
        self.assertIn("missing required byte size", stderr_buf.getvalue())

    def test_malformed_offset_fails(self):
        proc = subprocess.CompletedProcess(
            ["readelf", "--debug-dump=info", "test_binary"],
            returncode=0,
            stdout=MALFORMED_OFFSET_DWARF_OUTPUT,
            stderr="",
        )
        stderr_buf = io.StringIO()
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stderr(stderr_buf):
                with self.assertRaises(SystemExit) as cm:
                    extractor.main("test_binary")
        self.assertEqual(cm.exception.code, 1)
        self.assertIn("missing required field(s)", stderr_buf.getvalue())
        self.assertIn("stackguard0", stderr_buf.getvalue())


if __name__ == "__main__":
    unittest.main()
