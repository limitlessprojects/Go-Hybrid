package xfer_test

import (
	"math/big"
	"testing"

	"wtransfer/xfer"
)

// TestCallbackCanViolateWritePartition is the sharpest difference between the two
// call shapes, and it is not about mesh identity.
//
// The callback shape hands the caller a destination index. Nothing stops the
// callback from also writing some *other* row through that same slice, and the
// run's accounting can not see it: the other row is claimed by its own worker
// later (or earlier), so Finish still reports a complete, non-overlapping sweep
// while the field is wrong. The generated-loop shape can not express this at all:
// its body has no index to choose.
func TestCallbackCanViolateWritePartition(t *testing.T) {
	src := xfer.MustMesh("src", 0, 1, 3, 4)
	dst := xfer.MustMesh("dst", 0, 2, 4)
	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		t.Fatalf("BuildPlan: %v", err)
	}
	rho := xfer.RatField(3, 1, 2)
	want, err := xfer.IntegralResample(src, dst, rho)
	if err != nil {
		t.Fatalf("oracle: %v", err)
	}

	rogue := big.NewRat(999, 1)
	dstVal := xfer.ZeroField(dst.Cells())
	run := plan.NewRun()

	// worker 1 writes its own row first
	if _, err := run.ApplyRows(1, 2, src, dst, rho, func(i int, v *big.Rat) error {
		dstVal[i].Set(v)
		return nil
	}); err != nil {
		t.Fatalf("worker 1: %v", err)
	}
	// worker 0 writes its own row, and also row 1 which it does not own
	if _, err := run.ApplyRows(0, 2, src, dst, rho, func(i int, v *big.Rat) error {
		dstVal[i].Set(v)
		dstVal[1].Set(rogue) // the index it was handed is not the only one it has
		return nil
	}); err != nil {
		t.Fatalf("worker 0: %v", err)
	}

	// the run believes it is complete and disjoint
	if err := run.Finish(); err != nil {
		t.Fatalf("run accounting rejected the sweep: %v", err)
	}
	if xfer.EqualFields(dstVal, want) {
		t.Fatalf("fixture did not expose the rogue write")
	}
	t.Logf("callback API: row 1 was overwritten by worker 0, Finish() still passes; "+
		"field %v, oracle %v", xfer.FieldStrings(dstVal), xfer.FieldStrings(want))

	// the same mistake through the generated-loop shape: there is no index
	// variable to write through, so the only writes are the owned ones
	clean := xfer.ZeroField(dst.Cells())
	run2 := plan.NewRun()
	for w := 1; w >= 0; w-- {
		if _, err := run2.GatherRows(w, 2, src, dst, rho, clean); err != nil {
			t.Fatalf("gather worker %d: %v", w, err)
		}
	}
	if err := run2.Finish(); err != nil {
		t.Fatalf("gather finish: %v", err)
	}
	if !xfer.EqualFields(clean, want) {
		t.Fatalf("gather shape produced %v, oracle %v", xfer.FieldStrings(clean), xfer.FieldStrings(want))
	}
}
