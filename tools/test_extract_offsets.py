"""Readelf DWARF name forms accepted by the pinned-offset extractor."""

import contextlib
import io
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from tools import extract_offsets


def dwarf_output(name_prefix):
    return "\n".join(
        [
            " <1><10>: Abbrev Number: 3 (DW_TAG_structure_type)",
            f"    <11>   DW_AT_name        : {name_prefix}runtime.g",
            "    <12>   DW_AT_byte_size   : 64",
            " <2><20>: Abbrev Number: 4 (DW_TAG_member)",
            f"    <21>   DW_AT_name        : {name_prefix}stack",
            "    <22>   DW_AT_data_member_location: 0",
            " <2><23>: Abbrev Number: 4 (DW_TAG_member)",
            f"    <24>   DW_AT_name        : {name_prefix}stackguard0",
            "    <25>   DW_AT_data_member_location: 16",
            " <2><26>: Abbrev Number: 4 (DW_TAG_member)",
            f"    <27>   DW_AT_name        : {name_prefix}m",
            "    <28>   DW_AT_data_member_location: 48",
            " <1><30>: Abbrev Number: 3 (DW_TAG_structure_type)",
            f"    <31>   DW_AT_name        : {name_prefix}runtime.m",
            "    <32>   DW_AT_byte_size   : 80",
            " <2><40>: Abbrev Number: 4 (DW_TAG_member)",
            f"    <41>   DW_AT_name        : {name_prefix}g0",
            "    <42>   DW_AT_data_member_location: 8",
            " <2><43>: Abbrev Number: 4 (DW_TAG_member)",
            f"    <44>   DW_AT_name        : {name_prefix}curg",
            "    <45>   DW_AT_data_member_location: 16",
            " <2><46>: Abbrev Number: 4 (DW_TAG_member)",
            f"    <47>   DW_AT_name        : {name_prefix}id",
            "    <48>   DW_AT_data_member_location: 32",
            " <2><49>: Abbrev Number: 4 (DW_TAG_member)",
            f"    <4a>   DW_AT_name        : {name_prefix}locks",
            "    <4b>   DW_AT_data_member_location: 40",
        ]
    )


class ExtractOffsetsTests(unittest.TestCase):
    def test_plain_and_indirect_string_names_produce_identical_offsets(self):
        for prefix in ("", "(indirect string, offset: 0x123): ", "(indexed string: 0x3): "):
            with self.subTest(prefix=prefix):
                stdout = io.StringIO()
                with patch.object(
                    extract_offsets.subprocess,
                    "run",
                    return_value=SimpleNamespace(stdout=dwarf_output(prefix)),
                ), contextlib.redirect_stdout(stdout):
                    extract_offsets.main("local-go-binary")
                result = stdout.getvalue()
                self.assertIn("runtime.g size=64  runtime.m size=80", result)
                self.assertIn("g_stackguard0    = 16", result)
                self.assertIn("m_g0             = 8", result)


if __name__ == "__main__":
    unittest.main()
