package xfer_test

import (
	"fmt"
	"math/big"

	"wtransfer/xfer"
)

// The two examples are the call sites the issue asks to compare. They are
// runnable, so `go test` checks that both produce the same field.

// BEGIN CALLER: callback shape (library hands over a destination index)
func ExampleRun_ApplyRows() {
	src := xfer.MustMesh("src", 0, 1, 3)
	dst := xfer.MustMesh("dst", 0, 2, 3)
	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		panic(err)
	}
	rho := xfer.RatField(4, 1)
	out := xfer.ZeroField(dst.Cells())

	run := plan.NewRun()
	_, err = run.ApplyRows(0, 1, src, dst, rho, func(i int, v *big.Rat) error {
		out[i].Set(v) // the caller picks the destination slot
		return nil
	})
	if err != nil {
		panic(err)
	}
	if err := run.Finish(); err != nil {
		panic(err)
	}
	fmt.Println(xfer.FieldStrings(out))
	// Output: [5/2 1]
}

// END CALLER: callback shape

// BEGIN CALLER: generated-loop shape (the body only sees its own rows)
func ExampleRun_GatherRows() {
	src := xfer.MustMesh("src", 0, 1, 3)
	dst := xfer.MustMesh("dst", 0, 2, 3)
	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		panic(err)
	}
	rho := xfer.RatField(4, 1)
	out := xfer.ZeroField(dst.Cells())

	run := plan.NewRun()
	for worker := 0; worker < 2; worker++ {
		if _, err := run.GatherRows(worker, 2, src, dst, rho, out); err != nil {
			panic(err)
		}
	}
	if err := run.Finish(); err != nil {
		panic(err)
	}
	fmt.Println(xfer.FieldStrings(out))
	// Output: [5/2 1]
}

// END CALLER: generated-loop shape
