"""Regression tests for spikes/spike3-memory/work/extract_offsets.py DWARF offset extraction."""
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

    def _run(self, text):
        proc = subprocess.CompletedProcess(["readelf"], returncode=0, stdout=text, stderr="")
        out, err = io.StringIO(), io.StringIO()
        code = 0
        with patch.object(extractor.subprocess, "run", return_value=proc):
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                try:
                    extractor.main("test_binary")
                except SystemExit as e:
                    code = e.code
        return code, out.getvalue(), err.getvalue()

    def test_hex_and_plus_uconst_offsets_parse(self):
        text = (VALID_DWARF_OUTPUT
                .replace("DW_AT_data_member_location: 16", "DW_AT_data_member_location: 0x10")
                .replace("DW_AT_byte_size   : 376", "DW_AT_byte_size   : 0x178")
                .replace("DW_AT_data_member_location: 48",
                         "DW_AT_data_member_location: 2 byte block: 23 30 \t(DW_OP_plus_uconst: 48)"))
        code, out, _ = self._run(text)
        self.assertEqual(code, 0)
        self.assertIn("g_stackguard0    = 16", out)
        self.assertIn("g_m              = 48", out)
        self.assertIn("runtime.g size=376", out)

    def test_malformed_offset_fails_closed(self):
        text = VALID_DWARF_OUTPUT.replace("DW_AT_data_member_location: 16",
                                          "DW_AT_data_member_location: 16_corrupt")
        code, _, err = self._run(text)
        self.assertNotEqual(code, 0)
        self.assertIn("runtime.g.stackguard0", err)

    def test_missing_or_invalid_byte_size_fails(self):
        size_line = "    DW_AT_byte_size   : 376\n"
        for repl in ("", "    DW_AT_byte_size   : bogus\n"):
            text = VALID_DWARF_OUTPUT.replace(size_line, repl)
            code, _, err = self._run(text)
            self.assertNotEqual(code, 0)
            self.assertIn("runtime.g DW_AT_byte_size", err)

    def test_forward_declaration_is_skipped(self):
        fwd = (" <1><50>: Abbrev Number: 1 (DW_TAG_structure_type)\n"
               "    DW_AT_name        : runtime.g\n"
               "    DW_AT_declaration : 1\n")
        code, out, _ = self._run(fwd + VALID_DWARF_OUTPUT)
        self.assertEqual(code, 0)
        self.assertIn("g_stackguard0    = 16", out)

    def test_nested_members_do_not_clobber_outer(self):
        nested = (" <3><125>: Abbrev Number: 2 (DW_TAG_member)\n"
                  "    DW_AT_name        : stackguard0\n"
                  "    DW_AT_data_member_location: 999\n")
        anchor = " <2><130>: Abbrev Number: 2 (DW_TAG_member)"
        text = VALID_DWARF_OUTPUT.replace(anchor, nested + anchor, 1)
        code, out, _ = self._run(text)
        self.assertEqual(code, 0)
        self.assertIn("g_stackguard0    = 16", out)


if __name__ == "__main__":
    unittest.main()
