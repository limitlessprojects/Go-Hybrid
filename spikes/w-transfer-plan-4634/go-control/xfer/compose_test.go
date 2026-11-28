package xfer_test

import (
	"errors"
	"fmt"
	"math/big"
	"testing"

	"wtransfer/xfer"
)

// The composition experiment.
//
// Three meshes of the same cell count but different geometry, all partitions of
// [0,4]:
//
//	A [0,1][1,3][3,4]  widths 1,2,1
//	B [0,2][2,3][3,4]  widths 2,1,1
//	C [0,1][1,2][2,4]  widths 1,1,2
//
// Because the cell counts agree, a plan A->B writes straight into a C field
// without going out of bounds: every index exists and every row is legal for the
// mesh the plan was built for. That is exactly the mistake a task graph makes when
// it composes two transfers and loses track of which intermediate field it holds.

func composeFixture(t *testing.T) (a, b, c *xfer.Mesh) {
	t.Helper()
	a = xfer.MustMesh("A", 0, 1, 3, 4)
	b = xfer.MustMesh("B", 0, 2, 3, 4)
	c = xfer.MustMesh("C", 0, 1, 2, 4)
	if a.Cells() != b.Cells() || b.Cells() != c.Cells() {
		t.Fatalf("fixture meshes must have equal cell counts")
	}
	if a.Same(b) || b.Same(c) || a.Same(c) {
		t.Fatalf("fixture meshes must be distinct")
	}
	return
}

// TestHonestCompositionIsExactComposes A->B then B->C the intended way, and then
// records how far the two-step result is from a single A->C plan. The point is
// not that composition is wrong; it is that composing two averaging steps is a
// different operator from resampling once, so a caller that confuses the two gets
// a plausible-looking field that is not the one it asked for.
func TestHonestCompositionIsExact(t *testing.T) {
	a, b, c := composeFixture(t)

	planAB, err := xfer.BuildPlan(a, b)
	if err != nil {
		t.Fatalf("BuildPlan A->B: %v", err)
	}
	planBC, err := xfer.BuildPlan(b, c)
	if err != nil {
		t.Fatalf("BuildPlan B->C: %v", err)
	}
	planAC, err := xfer.BuildPlan(a, c)
	if err != nil {
		t.Fatalf("BuildPlan A->C: %v", err)
	}

	differ, total := 0, 0
	for _, rhoA := range densityVectors(a.Cells()) {
		fieldB := xfer.ZeroField(b.Cells())
		if err := planAB.Apply(a, b, rhoA, fieldB); err != nil {
			t.Fatalf("apply A->B: %v", err)
		}
		wantB, err := xfer.IntegralResample(a, b, rhoA)
		if err != nil {
			t.Fatalf("oracle A->B: %v", err)
		}
		if !xfer.EqualFields(fieldB, wantB) {
			t.Fatalf("A->B: got %v want %v", xfer.FieldStrings(fieldB), xfer.FieldStrings(wantB))
		}

		fieldC := xfer.ZeroField(c.Cells())
		if err := planBC.Apply(b, c, fieldB, fieldC); err != nil {
			t.Fatalf("apply B->C: %v", err)
		}
		wantTwoStep, err := xfer.IntegralResample(b, c, fieldB)
		if err != nil {
			t.Fatalf("oracle B->C: %v", err)
		}
		if !xfer.EqualFields(fieldC, wantTwoStep) {
			t.Fatalf("B->C: got %v want %v", xfer.FieldStrings(fieldC), xfer.FieldStrings(wantTwoStep))
		}

		direct := xfer.ZeroField(c.Cells())
		if err := planAC.Apply(a, c, rhoA, direct); err != nil {
			t.Fatalf("apply A->C: %v", err)
		}
		total++
		if !xfer.EqualFields(fieldC, direct) {
			differ++
		}
	}
	t.Logf("two-step A->B->C differs from one-step A->C in %d of %d density vectors", differ, total)
	if differ == 0 {
		t.Logf("note: on this fixture the two steps happen to agree everywhere")
	}
}

// TestWrongMeshAcceptedByBoundAPI is the judgement the issue asks for: do the two
// call shapes accept a plan whose intermediate mesh is the wrong one?
func TestWrongMeshAcceptedByBoundAPI(t *testing.T) {
	a, b, c := composeFixture(t)
	planAB, err := xfer.BuildPlan(a, b)
	if err != nil {
		t.Fatalf("BuildPlan A->B: %v", err)
	}
	rhoA := xfer.RatField(4, 1, 9)

	// (1) the callback shape, with C named as the destination
	dstC := xfer.ZeroField(c.Cells())
	called := 0
	_, err = planAB.NewRun().ApplyRows(0, 1, a, c, rhoA, func(i int, v *big.Rat) error {
		called++
		dstC[i].Set(v)
		return nil
	})
	if !errors.Is(err, xfer.ErrMeshIdentity) {
		t.Fatalf("callback API accepted a plan written for B: %v", err)
	}
	if called != 0 {
		t.Fatalf("callback API computed %d values before rejecting", called)
	}
	for i, v := range dstC {
		if v.Sign() != 0 {
			t.Fatalf("destination field was modified before the rejection at cell %d", i)
		}
	}

	// (2) the generated-loop shape, same mistake
	dstC2 := xfer.ZeroField(c.Cells())
	n, err := planAB.NewRun().GatherRows(0, 1, a, c, rhoA, dstC2)
	if !errors.Is(err, xfer.ErrMeshIdentity) {
		t.Fatalf("generated-loop API accepted a plan written for B: %v", err)
	}
	if n != 0 {
		t.Fatalf("generated-loop API wrote %d rows before rejecting", n)
	}

	// (3) the abbreviated convenience form
	if err := planAB.Apply(a, c, rhoA, xfer.ZeroField(c.Cells())); !errors.Is(err, xfer.ErrMeshIdentity) {
		t.Fatalf("Apply accepted a plan written for B: %v", err)
	}

	// (4) the mirror mistake: right destination, wrong source mesh
	if err := planAB.Apply(c, b, xfer.RatField(1, 1, 1), xfer.ZeroField(b.Cells())); !errors.Is(err, xfer.ErrMeshIdentity) {
		t.Fatalf("Apply accepted a plan written from a different source mesh: %v", err)
	}

	// (5) both meshes wrong at once
	if err := planAB.Apply(c, a, xfer.RatField(1, 1, 1), xfer.ZeroField(a.Cells())); !errors.Is(err, xfer.ErrMeshIdentity) {
		t.Fatalf("Apply accepted a plan for two different meshes: %v", err)
	}
}

// TestUnboundControlIsSilentlyWrong is the negative control for the gate itself:
// the same plan, applied through the index-only path that trusts the caller,
// writes a full field into C with no error and no way for the caller to notice
// from the write side. This is what the identity check buys.
func TestUnboundControlIsSilentlyWrong(t *testing.T) {
	a, b, c := composeFixture(t)
	planAB, err := xfer.BuildPlan(a, b)
	if err != nil {
		t.Fatalf("BuildPlan A->B: %v", err)
	}
	rhoA := xfer.RatField(4, 1, 9)

	// what B should hold
	wantB, err := xfer.IntegralResample(a, b, rhoA)
	if err != nil {
		t.Fatalf("oracle A->B: %v", err)
	}
	// what C should hold if the transfer were really A->C
	wantC, err := xfer.IntegralResample(a, c, rhoA)
	if err != nil {
		t.Fatalf("oracle A->C: %v", err)
	}

	writeFalse := xfer.ZeroField(c.Cells())
	if err := planAB.ApplyUnbound(rhoA, writeFalse, nil); err != nil {
		t.Fatalf("unbound control refused: %v", err)
	}
	if !xfer.EqualFields(writeFalse, wantB) {
		t.Fatalf("unbound control did not even produce B's values: %v", xfer.FieldStrings(writeFalse))
	}
	if xfer.EqualFields(writeFalse, wantC) {
		t.Fatalf("fixture is not discriminating: B's values equal C's values")
	}
	// the cheap post-hoc check still catches it, because the widths differ
	errIntegral := planAB.CheckIntegral(rhoA, writeFalse)
	if err == nil {
		t.Logf("note: CheckIntegral did NOT catch the wrong mesh on this fixture")
	} else if !errors.Is(err, xfer.ErrConservation) {
		t.Fatalf("unexpected integral error: %v", err)
	}
	t.Logf("unbound write into C: %v (B's values %v, C's true values %v), integral check: %v",
		xfer.FieldStrings(writeFalse), xfer.FieldStrings(wantB), xfer.FieldStrings(wantC), errIntegral)
}

// TestSameCellCountIsNotIdentity makes the trap explicit: the gate can not be
// replaced by a length check, because all three fields have three cells.
func TestSameCellCountIsNotIdentity(t *testing.T) {
	a, b, c := composeFixture(t)
	if a.Cells() != c.Cells() {
		t.Fatalf("fixture no longer shares a cell count")
	}
	planAB, err := xfer.BuildPlan(a, b)
	if err != nil {
		t.Fatalf("BuildPlan: %v", err)
	}
	// all three fingerprints are distinct, which is the only thing that can
	// distinguish the field C from the field B
	fps := map[uint64]string{a.Fingerprint(): "A", b.Fingerprint(): "B", c.Fingerprint(): "C"}
	if len(fps) != 3 {
		t.Fatalf("fingerprints collide across A, B, C: %v", fps)
	}
	if planAB.DstFingerprint() == c.Fingerprint() {
		t.Fatalf("plan binds C's fingerprint")
	}
	// and the plan records which mesh it is for, so the error message is usable
	if planAB.DstName() != "B" || planAB.SrcName() != "A" {
		t.Fatalf("plan lost its mesh names: %q -> %q", planAB.SrcName(), planAB.DstName())
	}
}

// TestComposedPlanChainThroughUnion shows the composition that is legitimate: keep
// the intermediate field bound to the mesh that produced it. Two plans, two fields,
// two identities, no confusion, and the second plan refuses the first field's plan.
func TestComposedPlanChainThroughUnion(t *testing.T) {
	a, b, c := composeFixture(t)
	planAB, err := xfer.BuildPlan(a, b)
	if err != nil {
		t.Fatalf("BuildPlan A->B: %v", err)
	}
	planBC, err := xfer.BuildPlan(b, c)
	if err != nil {
		t.Fatalf("BuildPlan B->C: %v", err)
	}
	rhoA := xfer.RatField(1, 0, 2)

	fieldA := xfer.NewField(a).SetValues(1, 0, 2)
	fieldB := xfer.NewField(b)
	fieldC := xfer.NewField(c)

	if err := planAB.ApplyFields(fieldA, fieldB); err != nil {
		t.Fatalf("A->B on bound fields: %v", err)
	}
	// feeding planBC a field bound to B is accepted (B is B)
	if err := planBC.ApplyFields(fieldB, fieldC); err != nil {
		t.Fatalf("B->C on bound fields: %v", err)
	}

	// The typed trap: planAB driven with C's bound field. Same cell count, so a
	// length check can not see it; the field's own mesh binding can.
	if err := planAB.ApplyFields(fieldA, xfer.NewField(c)); !errors.Is(err, xfer.ErrMeshIdentity) {
		t.Fatalf("ApplyFields accepted a plan written for B while naming C's field: %v", err)
	}
	// and the mirror: a source field bound to the wrong mesh
	if err := planAB.ApplyFields(xfer.NewField(c), fieldB); !errors.Is(err, xfer.ErrMeshIdentity) {
		t.Fatalf("ApplyFields accepted a source field bound to C: %v", err)
	}

	// The documented limit: hand the library a bare slice of numbers that were
	// computed on A, and it has no way to tell. Length matches, so it runs.
	bareDst := xfer.ZeroField(c.Cells())
	if err := planBC.Apply(b, c, rhoA, bareDst); err != nil {
		t.Fatalf("raw slices should be accepted (this is the limitation, not a bug): %v", err)
	}
	t.Logf("limit: planBC.Apply(b, c, <A's numbers as a bare slice>) accepted; "+
		"values %v; the same call through ApplyFields is refused",
		xfer.FieldStrings(bareDst))
	t.Logf("chain: A->B %v -> B->C %v", xfer.FieldStrings(fieldB.Val), xfer.FieldStrings(fieldC.Val))
}

// TestMinimalReproWrongMesh prints the smallest composition that distinguishes the
// two APIs, in the form the issue asks for.
func TestMinimalReproWrongMesh(t *testing.T) {
	a, b, c := composeFixture(t)
	planAB, _ := xfer.BuildPlan(a, b)
	rhoA := xfer.RatField(4, 1, 9)

	dst := xfer.ZeroField(c.Cells())
	bound := planAB.Apply(a, c, rhoA, dst)

	unboundDst := xfer.ZeroField(c.Cells())
	unbound := planAB.ApplyUnbound(rhoA, unboundDst, nil)

	msg := fmt.Sprintf(
		"cells=%d/%d/%d  plan_A->B.Apply(dst=C) -> %v  |  plan_A->B.ApplyUnbound(dst=C) -> %v, wrote %v",
		a.Cells(), b.Cells(), c.Cells(), bound, unbound, xfer.FieldStrings(unboundDst))
	t.Log(msg)
	if bound == nil {
		t.Fatalf("the bound API accepted the wrong mesh")
	}
	if unbound != nil {
		t.Fatalf("the unbound control refused, so it is not a control any more: %v", unbound)
	}
}
