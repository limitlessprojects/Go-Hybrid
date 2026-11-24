"""Tests for W-4634: Mesh Transfer Plan Carrying Write Partition and Conservation Law."""

import os
import sys
import unittest
from fractions import Fraction
from itertools import combinations, product

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from transfer_plan import (
    row_plan,
    validate_plan,
    run_partitioned,
    total_integral,
    cell_oracle,
    cells,
)


class TestMeshTransferPlan(unittest.TestCase):
    def test_density_example_preserves_integral(self):
        """Move field [4, 1] from mesh [0, 1, 3] to [0, 2, 3] preserves integral = 6."""
        src_mesh = (0, 1, 3)
        dst_mesh = (0, 2, 3)
        density = [4, 1]

        rows = row_plan(src_mesh, dst_mesh)
        self.assertTrue(validate_plan(src_mesh, dst_mesh, rows))

        out = run_partitioned(rows, density, workers=2)
        expected = [Fraction(5, 2), Fraction(1, 1)]  # [2.5, 1.0]
        self.assertEqual(out, expected)

        # Integrals match exactly 6
        self.assertEqual(total_integral(src_mesh, density), 6)
        self.assertEqual(total_integral(dst_mesh, out), 6)

    def test_falsifier_swapped_plan_rejected(self):
        """Equal width cells with swapped rows preserve total but fail spatial validation."""
        mesh = (0, 1, 2)
        swapped_rows = [[Fraction(0), Fraction(1)], [Fraction(1), Fraction(0)]]
        density = [1, 4]

        # Fails structural plan validation
        self.assertFalse(validate_plan(mesh, mesh, swapped_rows))

        # Output disagrees with raster oracle
        out = run_partitioned(swapped_rows, density, workers=2)
        oracle = cell_oracle(mesh, mesh, density)
        self.assertNotEqual(out, oracle)

    def test_exhaustive_mesh_combinations(self):
        """Exhaustive check on all partitions of [0, 4] with integer cuts."""
        interior_points = (1, 2, 3)
        meshes = [(0, *cut, 4) for k in range(4) for cut in combinations(interior_points, k)]

        verified_cases = 0
        for source, dest in product(meshes, repeat=2):
            rows = row_plan(source, dest)
            self.assertTrue(validate_plan(source, dest, rows))

            for rho in product((0, 1, 4), repeat=len(source) - 1):
                oracle = cell_oracle(source, dest, list(rho))
                for workers in (1, 2, 3):
                    out = run_partitioned(rows, list(rho), workers)
                    self.assertEqual(out, oracle)
                    self.assertEqual(total_integral(dest, out), total_integral(source, list(rho)))
                verified_cases += 1

        self.assertEqual(verified_cases, 1536)


if __name__ == "__main__":
    unittest.main()
