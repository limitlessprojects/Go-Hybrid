package xfer_test

import (
	"errors"
	"fmt"
	"math/big"
	"testing"

	"wtransfer/xfer"
)

// ---------------------------------------------------------------- fixtures

// allMeshes enumerates every partition of [0,4] at a subset of the integer
// interior points {1,2,3}: 2^3 = 8 meshes, the same enumeration as the issue.
func allMeshes() []*xfer.Mesh {
	interior := []int64{1, 2, 3}
	out := make([]*xfer.Mesh, 0, 1<<len(interior))
	for mask := 0; mask < 1<<len(interior); mask++ {
		edges := []int64{0}
		name := ""
		for i, p := range interior {
			if mask&(1<<i) != 0 {
				edges = append(edges, p)
				name += fmt.Sprint(p)
			}
		}
		edges = append(edges, 4)
		if name == "" {
			name = "none"
		}
		out = append(out, xfer.MustMesh("m["+name+"]", edges...))
	}
	return out
}

// densityVectors enumerates {0,1,4}^cells.
func densityVectors(cells int) [][]*big.Rat {
	choices := []int64{0, 1, 4}
	total := 1
	for i := 0; i < cells; i++ {
		total *= len(choices)
	}
	out := make([][]*big.Rat, 0, total)
	for c := 0; c < total; c++ {
		f := make([]*big.Rat, cells)
		x := c
		for i := 0; i < cells; i++ {
			f[i] = new(big.Rat).SetInt64(choices[x%len(choices)])
			x /= len(choices)
		}
		out = append(out, f)
	}
	return out
}

// tableSweep applies a plan over every worker in turn, in reverse order, and
// returns the partition coverage that the run recorded. Reverse order is
// deliberate: if two workers overlapped, the earlier worker's value would be
// overwritten and the mismatch would show in the oracle comparison instead of
// staying hidden behind a lucky ordering.
func tableSweep(t *testing.T, plan *xfer.Plan, src, dst *xfer.Mesh,
	rho []*big.Rat, workers int, useCallback bool) []*big.Rat {
	t.Helper()
	dstVal := xfer.ZeroField(dst.Cells())
	run := plan.NewRun()
	totalWrites := 0
	for w := workers - 1; w >= 0; w-- {
		var n int
		var err error
		if useCallback {
			n, err = run.ApplyRows(w, workers, src, dst, rho, func(i int, v *big.Rat) error {
				dstVal[i].Set(v)
				return nil
			})
		} else {
			n, err = run.GatherRows(w, workers, src, dst, rho, dstVal)
		}
		if err != nil {
			t.Fatalf("worker %d/%d: %v", w, workers, err)
		}
		totalWrites += n
	}
	if err := run.Finish(); err != nil {
		t.Fatalf("finish: %v", err)
	}
	if totalWrites != dst.Cells() {
		t.Fatalf("workers wrote %d rows, destination has %d cells", totalWrites, dst.Cells())
	}
	if run.Writes() != dst.Cells() {
		t.Fatalf("run counted %d writes, destination has %d cells", run.Writes(), dst.Cells())
	}
	return dstVal
}

// -------------------------------------------------------------- test 1: the issue's numbers

// TestExhaustiveOracleMatch is the main experiment. For every ordered pair of
// meshes and every density vector, both call shapes must reproduce the
// independent oracle cell by cell, and the plan must pass its own validation
// and the conservation law.
func TestExhaustiveOracleMatch(t *testing.T) {
	meshes := allMeshes()
	if len(meshes) != 8 {
		t.Fatalf("expected 8 meshes, got %d", len(meshes))
	}

	cases, layouts := 0, 0
	oracleDisagreements := 0
	plans := 0
	entries := 0

	for _, src := range meshes {
		for _, dst := range meshes {
			plan, err := xfer.BuildPlan(src, dst)
			if err != nil {
				t.Fatalf("BuildPlan(%s->%s): %v", src, dst, err)
			}
			if err := plan.Validate(); err != nil {
				t.Fatalf("Validate(%s->%s): %v", src, dst, err)
			}
			plans++
			entries += plan.EntryCount()

			for _, rho := range densityVectors(src.Cells()) {
				want, err := xfer.IntegralResample(src, dst, rho)
				if err != nil {
					t.Fatalf("oracle: %v", err)
				}
				raster, err := xfer.RasterResample(src, dst, rho)
				if err != nil {
					t.Fatalf("raster oracle: %v", err)
				}
				if !xfer.EqualFields(want, raster) {
					oracleDisagreements++
					t.Fatalf("oracles disagree on %s->%s rho=%v: analytic %v raster %v",
						src, dst, xfer.FieldStrings(rho), xfer.FieldStrings(want), xfer.FieldStrings(raster))
				}

				for _, workers := range []int{1, 2, 3} {
					for _, useCallback := range []bool{true, false} {
						got := tableSweep(t, plan, src, dst, rho, workers, useCallback)
						if !xfer.EqualFields(got, want) {
							t.Fatalf("%s->%s rho=%v workers=%d callback=%v: got %v want %v",
								src, dst, xfer.FieldStrings(rho), workers, useCallback,
								xfer.FieldStrings(got), xfer.FieldStrings(want))
						}
						if err := plan.CheckIntegral(rho, got); err != nil {
							t.Fatalf("conservation %s->%s rho=%v workers=%d callback=%v: %v",
								src, dst, xfer.FieldStrings(rho), workers, useCallback, err)
						}
					}
					layouts++
				}
				cases++
			}
		}
	}
	t.Logf("plans=%d entries=%d cases=%d layouts=%d oracle_disagreements=%d",
		plans, entries, cases, layouts, oracleDisagreements)
	// The issue claims 1536 mesh/data cases and 4608 worker layouts for
	// workers in {1,2,3}. Both call shapes are exercised inside each layout.
	if cases != 1536 {
		t.Fatalf("mesh/data cases: got %d, issue claims 1536", cases)
	}
	if layouts != 4608 {
		t.Fatalf("worker layouts: got %d, issue claims 4608", layouts)
	}
	if oracleDisagreements != 0 {
		t.Fatalf("oracle disagreements: %d", oracleDisagreements)
	}
}

// TestExhaustiveFourWorkers is the same sweep with 4 workers, a worker count the
// Python model never ran. It is reported separately so the 4608 figure above
// stays comparable with the issue.
func TestExhaustiveFourWorkers(t *testing.T) {
	meshes := allMeshes()
	cases, layouts := 0, 0
	for _, src := range meshes {
		for _, dst := range meshes {
			plan, err := xfer.BuildPlan(src, dst)
			if err != nil {
				t.Fatalf("BuildPlan: %v", err)
			}
			for _, rho := range densityVectors(src.Cells()) {
				want, err := xfer.IntegralResample(src, dst, rho)
				if err != nil {
					t.Fatalf("oracle: %v", err)
				}
				for _, useCallback := range []bool{true, false} {
					got := tableSweep(t, plan, src, dst, rho, 4, useCallback)
					if !xfer.EqualFields(got, want) {
						t.Fatalf("%s->%s rho=%v workers=4 callback=%v: got %v want %v",
							src, dst, xfer.FieldStrings(rho), useCallback,
							xfer.FieldStrings(got), xfer.FieldStrings(want))
					}
				}
				layouts++
				cases++
			}
		}
	}
	t.Logf("cases=%d layouts(workers=4)=%d", cases, layouts)
	if cases != 1536 || layouts != 1536 {
		t.Fatalf("4-worker sweep: cases=%d layouts=%d, expected 1536/1536", cases, layouts)
	}
}

// TestIssueHeadlineExample pins the two examples the issue uses to argue that
// geometry, not the total, is the real constraint.
func TestIssueHeadlineExample(t *testing.T) {
	src := xfer.MustMesh("src", 0, 1, 3)
	dst := xfer.MustMesh("dst", 0, 2, 3)
	rho := xfer.RatField(4, 1)

	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		t.Fatalf("BuildPlan: %v", err)
	}
	got := xfer.ZeroField(dst.Cells())
	if err := plan.Apply(src, dst, rho, got); err != nil {
		t.Fatalf("Apply: %v", err)
	}
	want := []string{"5/2", "1"}
	if fmt.Sprint(xfer.FieldStrings(got)) != fmt.Sprint(want) {
		t.Fatalf("overlap-weighted result %v, want %v", xfer.FieldStrings(got), want)
	}
	srcTotal, _ := xfer.IntegralOf(src, rho)
	dstTotal, _ := xfer.IntegralOf(dst, got)
	if srcTotal.Cmp(dstTotal) != 0 {
		t.Fatalf("integral changed: %s -> %s", srcTotal.RatString(), dstTotal.RatString())
	}
	if srcTotal.RatString() != "6" {
		t.Fatalf("source integral %s, issue says 6", srcTotal.RatString())
	}

	// Negative control A: copy by cell index. The write schedule is perfectly
	// disjoint and the cell count is unchanged, but the integral moves 6 -> 9.
	byIndex := make([]xfer.Row, dst.Cells())
	for j := range byIndex {
		byIndex[j] = xfer.Row{Dst: j, Width: dst.Width(j),
			Entries: []xfer.Entry{{Src: j, W: *xfer.RatOf(1)}}}
	}
	copied := xfer.ZeroField(dst.Cells())
	for j, row := range byIndex {
		copied[j].Set(rho[row.Entries[0].Src])
	}
	if fmt.Sprint(xfer.FieldStrings(copied)) != "[4 1]" {
		t.Fatalf("index copy produced %v", xfer.FieldStrings(copied))
	}
	copiedTotal, _ := xfer.IntegralOf(dst, copied)
	if copiedTotal.RatString() != "9" {
		t.Fatalf("index copy integral %s, issue says 9", copiedTotal.RatString())
	}
	if _, err := xfer.AdmitPlan(src, dst, byIndex); !errors.Is(err, xfer.ErrGeometry) {
		t.Fatalf("index-copy plan admitted or wrong error: %v", err)
	}

	// Negative control B: swap two equal-width rows on one mesh. The integral
	// survives, both cells are wrong, and the geometry check rejects it.
	mesh := xfer.MustMesh("unit", 0, 1, 2)
	swapped := []xfer.Row{
		{Dst: 0, Width: 1, Entries: []xfer.Entry{{Src: 1, W: *xfer.RatOf(1)}}},
		{Dst: 1, Width: 1, Entries: []xfer.Entry{{Src: 0, W: *xfer.RatOf(1)}}},
	}
	values := xfer.RatField(1, 4)
	wrong := xfer.ZeroField(2)
	for j, row := range swapped {
		wrong[j].Set(values[row.Entries[0].Src])
	}
	before, _ := xfer.IntegralOf(mesh, values)
	after, _ := xfer.IntegralOf(mesh, wrong)
	if before.Cmp(after) != 0 {
		t.Fatalf("swapped control did not preserve the integral: %s vs %s", before, after)
	}
	if xfer.FieldStrings(wrong)[0] == "1" {
		t.Fatalf("swapped control did not permute the cells: %v", xfer.FieldStrings(wrong))
	}
	if _, err := xfer.AdmitPlan(mesh, mesh, swapped); !errors.Is(err, xfer.ErrGeometry) {
		t.Fatalf("swapped plan admitted or wrong error: %v", err)
	}
	t.Logf("index copy: %v integral %s (disjoint writes, plan rejected); swapped rows: %v integral %s (plan rejected)",
		xfer.FieldStrings(copied), copiedTotal.RatString(),
		xfer.FieldStrings(wrong), after.RatString())
}

// TestPerturbedWeightRejected keeps a row's total measure constant while moving
// mass between source cells, so the write partition still looks legal. The
// per-entry geometry check is what refuses it.
func TestPerturbedWeightRejected(t *testing.T) {
	src := xfer.MustMesh("src", 0, 1, 3)
	dst := xfer.MustMesh("dst", 0, 2, 3)
	rows := []xfer.Row{
		{Dst: 0, Width: 2, Entries: []xfer.Entry{
			{Src: 0, W: *new(big.Rat).SetFrac64(1, 4)},
			{Src: 1, W: *new(big.Rat).SetFrac64(3, 4)},
		}},
		{Dst: 1, Width: 1, Entries: []xfer.Entry{{Src: 1, W: *xfer.RatOf(1)}}},
	}
	// the row still covers its cell exactly once: (1/4)*2 + (3/4)*2 == 2
	if _, err := xfer.AdmitPlan(src, dst, rows); !errors.Is(err, xfer.ErrGeometry) {
		t.Fatalf("perturbed plan admitted or wrong error: %v", err)
	}
}

// TestConservationLawRejected removes one source cell's share entirely: every
// row is still geometrically correct for the cells it does touch, but a source
// cell's measure is lost. Only the conservation clause catches this.
func TestConservationLawRejected(t *testing.T) {
	src := xfer.MustMesh("src", 0, 1, 2)
	dst := xfer.MustMesh("dst", 0, 2) // one cell, overlaps both source cells
	rows := []xfer.Row{{Dst: 0, Width: 2, Entries: []xfer.Entry{
		{Src: 0, W: *new(big.Rat).SetFrac64(1, 2)},
	}}}
	// row coverage is 1, cell width is 2 -> the partition clause fires first
	err := mustReject(t, src, dst, rows)
	if !errors.Is(err, xfer.ErrPartition) && !errors.Is(err, xfer.ErrConservation) {
		t.Fatalf("unexpected rejection reason: %v", err)
	}
}

func mustReject(t *testing.T, src, dst *xfer.Mesh, rows []xfer.Row) error {
	t.Helper()
	p, err := xfer.AdmitPlan(src, dst, rows)
	if err == nil {
		t.Fatalf("plan was admitted: %d rows", len(p.Rows()))
	}
	return err
}

// ------------------------------------------------------- test: partition guards

// TestWritePartitionGuards covers the "each destination cell is written exactly
// once" rule at runtime, not just at plan time.
func TestWritePartitionGuards(t *testing.T) {
	src := xfer.MustMesh("src", 0, 1, 3, 4)
	dst := xfer.MustMesh("dst", 0, 2, 4)
	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		t.Fatalf("BuildPlan: %v", err)
	}
	rho := xfer.RatField(1, 2, 3)

	// a worker claiming the same rows twice must be refused
	run := plan.NewRun()
	dstVal := xfer.ZeroField(dst.Cells())
	if _, err := run.GatherRows(0, 2, src, dst, rho, dstVal); err != nil {
		t.Fatalf("first sweep: %v", err)
	}
	if _, err := run.GatherRows(0, 2, src, dst, rho, dstVal); !errors.Is(err, xfer.ErrDoubleWrite) {
		t.Fatalf("second sweep on the same rows: %v", err)
	}

	// an incomplete sweep leaves rows unwritten
	run2 := plan.NewRun()
	if _, err := run2.GatherRows(0, 2, src, dst, rho, xfer.ZeroField(dst.Cells())); err != nil {
		t.Fatalf("partial sweep: %v", err)
	}
	if err := run2.Finish(); !errors.Is(err, xfer.ErrPartition) {
		t.Fatalf("incomplete sweep finished cleanly: %v", err)
	}

	// a sweep only touches its own rows
	dstPartial := xfer.ZeroField(dst.Cells())
	if _, err := plan.NewRun().GatherRows(1, 2, src, dst, rho, dstPartial); err != nil {
		t.Fatalf("worker 1 sweep: %v", err)
	}
	if dstPartial[0].Sign() != 0 {
		t.Fatalf("worker 1 wrote row 0, which it does not own: %v", xfer.FieldStrings(dstPartial))
	}

	// a field of the wrong length is refused instead of indexed out of range
	if err := plan.Apply(src, dst, xfer.RatField(1, 2), xfer.ZeroField(dst.Cells())); !errors.Is(err, xfer.ErrFieldLength) {
		t.Fatalf("short source field: %v", err)
	}
	if _, err := plan.NewRun().GatherRows(0, 1, src, dst, rho, xfer.ZeroField(dst.Cells()+1)); !errors.Is(err, xfer.ErrFieldLength) {
		t.Fatalf("long target field: %v", err)
	}

	// an unowned row view can not be stored through
	var v xfer.RowView
	if err := v.Store(dstVal); !errors.Is(err, xfer.ErrUnowned) {
		t.Fatalf("unowned view stored: %v", err)
	}
}

// TestMeshIdentityIsStructural checks that identity is the geometry, not the
// pointer: a mesh rebuilt from the same edges is the same mesh.
func TestMeshIdentityIsStructural(t *testing.T) {
	src := xfer.MustMesh("src", 0, 2, 4)
	dst := xfer.MustMesh("dst", 0, 1, 4)
	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		t.Fatalf("BuildPlan: %v", err)
	}
	srcCopy := xfer.MustMesh("src-again", 0, 2, 4)
	dstCopy := xfer.MustMesh("dst-again", 0, 1, 4)
	got := xfer.ZeroField(dst.Cells())
	if err := plan.Apply(srcCopy, dstCopy, xfer.RatField(1, 3), got); err != nil {
		t.Fatalf("same geometry rejected: %v", err)
	}
	if !src.Same(srcCopy) || src.Fingerprint() != srcCopy.Fingerprint() {
		t.Fatalf("fingerprint is not a function of the edges")
	}
	if src.Same(dst) {
		t.Fatalf("different meshes reported as the same mesh")
	}
}
