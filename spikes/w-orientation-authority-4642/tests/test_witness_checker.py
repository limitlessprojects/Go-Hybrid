"""Tests for the W-4642 static rule (witness_checker)."""

import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from witness_checker import CheckError, check_source  # noqa: E402

CASES = HERE.parent / "cases"


def expectation(path: pathlib.Path) -> tuple[str, str | None]:
    header = path.read_text(encoding="utf-8").splitlines()[0]
    parts = header.replace("#", "").strip().split()
    # "expect: accept" | "expect: reject HYB-E4644"
    verdict = parts[1]
    code = parts[2] if len(parts) > 2 else None
    return verdict, code


class CaseFiles(unittest.TestCase):
    def test_case_files_are_labelled(self):
        files = sorted(CASES.glob("*.hyc"))
        self.assertGreaterEqual(len(files), 10)
        for path in files:
            with self.subTest(case=path.name):
                verdict, _ = expectation(path)
                self.assertIn(verdict, {"accept", "reject"})

    def test_every_case_matches_its_header(self):
        for path in sorted(CASES.glob("*.hyc")):
            verdict, code = expectation(path)
            source = path.read_text(encoding="utf-8")
            with self.subTest(case=path.name):
                if verdict == "accept":
                    check_source(source)  # must not raise
                else:
                    with self.assertRaises(CheckError) as raised:
                        check_source(source)
                    self.assertEqual(raised.exception.code, code)

    def test_positive_cases_are_not_vacuous(self):
        """The accepting cases must actually reach an insert check."""
        accepted = [p for p in sorted(CASES.glob("*.hyc")) if expectation(p)[0] == "accept"]
        self.assertGreaterEqual(len(accepted), 4)
        for path in accepted:
            self.assertIn("insert(", path.read_text(encoding="utf-8"))


class RuleDetails(unittest.TestCase):
    def test_unindexed_helper_is_rejected_with_fix_it_hint(self):
        source = (CASES / "03-reject-helper-unindexed.hyc").read_text(encoding="utf-8")
        with self.assertRaises(CheckError) as raised:
            check_source(source)
        self.assertEqual(raised.exception.code, "HYB-E4645")
        self.assertIn("unindexed", raised.exception.message)

    def test_wrong_order_is_caught_at_the_signature_too(self):
        """A helper that promises (0,1,2) but observes (0,2,1) dies at the return."""
        source = """
fn classify(m: mesh) -> turn[m@m,(0,1,2)] {
    o = observe(m, 0, 2, 1)
    return orientation(o)
}

fn hull(m: mesh) {
    t = call classify(m)
    insert(m, 0, 1, 2, t)
}
"""
        with self.assertRaises(CheckError) as raised:
            check_source(source)
        self.assertEqual(raised.exception.code, "HYB-E4644")

    def test_stale_after_two_edits(self):
        source = """
fn hull(m: mesh) {
    o = observe(m, 0, 1, 2)
    t = orientation(o)
    edit(m)
    edit(m)
    insert(m, 0, 1, 2, t)
}
"""
        with self.assertRaises(CheckError) as raised:
            check_source(source)
        self.assertEqual(raised.exception.code, "HYB-E4643")
        self.assertIn("@1", raised.exception.message)
        self.assertIn("@3", raised.exception.message)

    def test_epoch_survives_unrelated_mesh_edit(self):
        """Editing *another* mesh must not invalidate this mesh's witness."""
        source = """
fn hull(m: mesh, n: mesh) {
    o = observe(m, 0, 1, 2)
    t = orientation(o)
    edit(n)
    insert(m, 0, 1, 2, t)
}
"""
        check_source(source)

    def test_recursive_helper_does_not_hang(self):
        source = """
fn f(m: mesh, i: int, j: int, k: int) -> turn[m@m,(i,j,k)] {
    t = call f(m, i, j, k)
    return t
}

fn hull(m: mesh) {
    t = call f(m, 0, 1, 2)
    insert(m, 0, 1, 2, t)
}
"""
        try:
            check_source(source)
        except CheckError:
            pass  # a rejection is fine; a hang or a RecursionError is not


class ShapeErrors(unittest.TestCase):
    def test_unknown_call_is_a_shape_error(self):
        with self.assertRaises(CheckError) as raised:
            check_source("fn hull(m: mesh) {\n    x = frobnicate(m)\n}\n")
        self.assertEqual(raised.exception.code, "HYB-E4646")

    def test_unterminated_function_is_a_shape_error(self):
        with self.assertRaises(CheckError) as raised:
            check_source("fn hull(m: mesh) {\n    o = observe(m, 0, 1, 2)\n")
        self.assertEqual(raised.exception.code, "HYB-E4646")


if __name__ == "__main__":
    unittest.main()
