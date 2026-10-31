#!/usr/bin/env python3
"""Unit tests for the Tier-0 borrow_ffi[T] zero-copy verifier and falsifiers."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from borrow_ffi_verifier import (  # noqa: E402
    BorrowFfiCallSite,
    PRE_REGISTERED_CASES,
    verify_borrow_ffi_site,
)


class BorrowFfiVerifierTests(unittest.TestCase):
    def test_pre_registered_hypotheses_and_falsifiers(self) -> None:
        for site, expected_ok, expected_code in PRE_REGISTERED_CASES:
            ok, diags = verify_borrow_ffi_site(site)
            self.assertEqual(ok, expected_ok, msg=f"{site.name}: {diags}")
            if expected_code is None:
                self.assertEqual(diags, [])
            else:
                self.assertTrue(any(expected_code in d for d in diags), msg=f"{site.name}: {diags}")

    def test_multiple_simultaneous_violations_reported_deterministically(self) -> None:
        site = BorrowFfiCallSite(
            name="compound_falsifier",
            view_escapes_via="return_handle",
            backing_mutated_in_window=True,
            is_subslice=True,
            three_index_clipped=False,
        )
        ok, diags = verify_borrow_ffi_site(site)
        self.assertFalse(ok)
        self.assertEqual(len(diags), 3)
        self.assertIn("HYB-E3501", diags[0])
        self.assertIn("HYB-E3502", diags[1])
        self.assertIn("HYB-E3503", diags[2])


if __name__ == "__main__":
    unittest.main()
