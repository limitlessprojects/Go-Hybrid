"""Tests for the W-4642 runtime half (orientation_authority)."""

import pathlib
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import orientation_authority as oa  # noqa: E402


class Predicate(unittest.TestCase):
    def test_textbook_cases(self):
        self.assertEqual(oa.oracle((0, 0), (1, 0), (0, 1)), oa.LEFT)
        self.assertEqual(oa.oracle((0, 0), (1, 0), (2, 0)), oa.COLLINEAR)
        self.assertEqual(oa.oracle((0, 0), (0, 1), (1, 0)), oa.RIGHT)
        for triangle in [((0, 0), (1, 0), (0, 1)), ((0, 0), (0, 1), (1, 0)), ((0, 0), (1, 0), (2, 0))]:
            self.assertEqual(oa.classify(*triangle)[0], oa.oracle(*triangle))
            self.assertEqual(oa.naive(*triangle), oa.oracle(*triangle))

    def test_unit_determinant_family_defeats_the_naive_implementation(self):
        cases = oa.family_unit_determinant()
        self.assertEqual(len(cases), 46)
        for triangle in cases:
            with self.subTest(triangle=triangle):
                self.assertEqual(abs(oa.oracle(*triangle)), 1)  # |determinant| = 1, both orders
                self.assertEqual(oa.naive(*triangle), oa.COLLINEAR)  # rounded to a tie
                direction, used_exact = oa.classify(*triangle)
                self.assertEqual(direction, oa.oracle(*triangle))
                self.assertTrue(used_exact, "the filter must not answer this family")

    def test_rounded_then_exact_cannot_recover_the_bit(self):
        cases = oa.family_unit_determinant()
        self.assertTrue(all(oa.exact_from_rounded(*t) != oa.oracle(*t) for t in cases))

    def test_degenerate_family(self):
        for triangle in oa.family_degenerate():
            self.assertEqual(oa.classify(*triangle)[0], oa.oracle(*triangle))

    def test_random_family_agrees_with_the_oracle(self):
        for triangle in oa.family_random(2000, 1 << 26, seed=99):
            self.assertEqual(oa.classify(*triangle)[0], oa.oracle(*triangle))

    def test_bound_is_enforced(self):
        with self.assertRaises(ValueError):
            oa.classify((0, 0), (1 << 51, 0), (0, 1))
        # the bound itself is still inside the exactly-representable difference range
        oa.classify((0, 0), (1 << 50, 0), (0, 1))

    def test_non_integer_coordinates_are_refused(self):
        with self.assertRaises(TypeError):
            oa.classify((0.5, 0), (1, 0), (0, 1))


class Authority(unittest.TestCase):
    def setUp(self):
        self.triple = ((3, -5), (3 + (1 << 27), -5 + (1 << 27) - 1), (3 + (1 << 27) + 1, -5 + (1 << 27)))
        self.certifier = oa.Certifier()

    def mesh(self, mesh_id=1):
        return oa.Mesh(mesh_id, list(self.triple))

    def test_honest_path_commits(self):
        m = self.mesh()
        witness = oa.classify_helper(self.certifier, m, (0, 1, 2))
        self.assertEqual(witness.direction, oa.LEFT)
        self.assertEqual(oa.commit_helper(m, (0, 1, 2), witness), oa.LEFT)

    def test_wrong_order_rejected(self):
        m = self.mesh()
        witness = self.certifier.observe(m, (0, 1, 2))
        with self.assertRaises(oa.StaleWitness):
            m.commit((0, 2, 1), witness)

    def test_stale_revision_rejected(self):
        m = self.mesh()
        witness = self.certifier.observe(m, (0, 1, 2))
        m.edit_point(2, (self.triple[2][0], self.triple[2][1] - 1))
        with self.assertRaises(oa.StaleWitness):
            m.commit((0, 1, 2), witness)

    def test_other_mesh_at_same_revision_rejected(self):
        a, b = self.mesh(1), self.mesh(2)
        witness = self.certifier.observe(a, (0, 1, 2))
        with self.assertRaises(oa.StaleWitness):
            b.commit((0, 1, 2), witness)

    def test_forged_witness_rejected(self):
        with self.assertRaises(oa.ForgedWitness):
            oa.Turn(mesh=1, revision=1, triple=(0, 1, 2), direction=1, coords_digest="0" * 16, _key=object())

    def test_coordinate_edit_without_revision_bump_is_caught(self):
        """Deeper than the proposal's revision check: the digest catches a silent write."""
        m = self.mesh()
        witness = self.certifier.observe(m, (0, 1, 2))
        m._points[2] = (self.triple[2][0], self.triple[2][1] - 1)  # noqa: SLF001
        with self.assertRaises(oa.StaleWitness):
            m.commit((0, 1, 2), witness)

    def test_sign_only_control_would_have_applied_the_wrong_edit(self):
        m = self.mesh()
        witness = self.certifier.observe(m, (0, 1, 2))
        correct = oa.oracle(self.triple[0], self.triple[2], self.triple[1])
        self.assertEqual(witness.direction, oa.LEFT)
        self.assertEqual(correct, oa.RIGHT)
        # the control applies it anyway; nothing in a bare sign stops it
        self.assertEqual(oa.sign_only_commit(m, (0, 2, 1), witness.direction), oa.LEFT)


if __name__ == "__main__":
    unittest.main()
