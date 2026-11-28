#!/usr/bin/env python3
"""Gates of wildcard proposal #4642: orientation evidence bound to its sites."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
import orientation_evidence as m  # noqa: E402

L = m.COORD_LIMIT


def mesh_with(points):
    return m.Mesh("m", {i: p for i, p in enumerate(points)})


class TestExactSign(unittest.TestCase):
    """H1 — the predicate itself is exact where float64 stops being exact."""

    def test_orientation_of_a_known_turn(self):
        self.assertEqual(m.exact_orientation((0, 0), (L, 0), (0, L)), 1)
        self.assertEqual(m.exact_orientation((0, 0), (0, L), (L, 0)), -1)
        self.assertEqual(m.exact_orientation((0, 0), (L, L), (2 * L, 2 * L)), 0)

    def test_float_is_safe_at_the_proposed_bound(self):
        bad, _ = m.bare_sign_failures(20000, limit=L)
        self.assertEqual(bad, 0)

    def test_float_breaks_two_bits_higher(self):
        bad, first = m.bare_sign_failures(20000, limit=1 << 53)
        self.assertGreater(bad, 100)
        p, q, r = first[0]
        self.assertNotEqual(m.exact_orientation(p, q, r), 0)
        self.assertEqual(m.float_orientation(p, q, r), 0)


class TestBinding(unittest.TestCase):
    """H2-H5 — evidence is only good for the sites it was made for."""

    def test_happy_path_returns_the_triple_and_bumps_revision(self):
        mesh = mesh_with([(0, 0), (L, 0), (0, L)])
        turn = m.orientation(m.observe(mesh, 0, 1, 2))
        self.assertEqual(m.insert_face(mesh, turn), (0, 1, 2))
        self.assertEqual(mesh.revision, 1)

    def test_reordered_triple_is_a_mismatch(self):
        mesh = mesh_with([(0, 0), (L, 0), (0, L)])
        turn = m.orientation(m.observe(mesh, 0, 1, 2))
        with self.assertRaises(m.EvidenceError) as ctx:
            m.insert_face(mesh, turn, face=(0, 2, 1))
        self.assertEqual(ctx.exception.code, m.E_TRIPLE_MISMATCH)

    def test_permutation_flips_the_sign_so_a_bare_sign_would_pass(self):
        mesh = mesh_with([(0, 0), (L, 0), (0, L)])
        forward = m.orientation(m.observe(mesh, 0, 1, 2))
        backward = m.orientation(m.observe(mesh, 0, 2, 1))
        self.assertNotEqual(forward.turn, backward.turn)

    def test_evidence_from_another_mesh_is_rejected(self):
        a = mesh_with([(0, 0), (L, 0), (0, L)])
        b = m.Mesh("other", a.coords.copy())
        turn = m.orientation(m.observe(a, 0, 1, 2))
        with self.assertRaises(m.EvidenceError) as ctx:
            m.insert_face(b, turn)
        self.assertEqual(ctx.exception.code, m.E_OTHER_MESH)

    def test_edited_coordinate_is_rejected(self):
        mesh = mesh_with([(0, 0), (L, 0), (0, L)])
        turn = m.orientation(m.observe(mesh, 0, 1, 2))
        mesh.move(2, 1, L)
        with self.assertRaises(m.EvidenceError) as ctx:
            m.insert_face(mesh, turn)
        self.assertEqual(ctx.exception.code, m.E_STALE_REVISION)

    def test_edited_coordinate_without_revision_bump_is_caught_by_the_digest(self):
        mesh = mesh_with([(0, 0), (L, 0), (0, L)])
        turn = m.orientation(m.observe(mesh, 0, 1, 2))
        mesh.coords[2] = (1, L)          # revision untouched on purpose
        with self.assertRaises(m.EvidenceError) as ctx:
            m.insert_face(mesh, turn)
        self.assertEqual(ctx.exception.code, m.E_MOVED_VERTEX)

    def test_forged_turn_is_rejected(self):
        mesh = mesh_with([(0, 0), (L, 0), (0, L)])
        obs = m.observe(mesh, 0, 1, 2)
        with self.assertRaises(m.EvidenceError) as ctx:
            m.insert_face(mesh, m.Turn(obs, m.RIGHT))
        self.assertEqual(ctx.exception.code, m.E_TURN_DISAGREES)

    def test_vertex_above_the_bound_needs_the_escape_hatch(self):
        mesh = mesh_with([(0, 0), (L, L), (0, 2 * L)])
        with self.assertRaises(m.EvidenceError) as ctx:
            m.observe(mesh, 0, 1, 2)
        self.assertEqual(ctx.exception.code, m.E_ESCAPED)


if __name__ == "__main__":
    unittest.main()
