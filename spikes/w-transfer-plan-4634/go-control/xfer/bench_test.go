package xfer_test

import (
	"math/big"
	"testing"

	"wtransfer/xfer"
)

// benchMeshes builds two n-cell partitions of [0, 3n] whose cells are offset by
// one unit, so nearly every destination cell straddles two source cells. That
// makes the plan non-trivial (about 2n entries) without making it O(n^2).
func benchMeshes(n int) (*xfer.Mesh, *xfer.Mesh) {
	srcEdges := make([]int64, 0, n+1)
	for i := 0; i <= n; i++ {
		srcEdges = append(srcEdges, int64(3*i))
	}
	dstEdges := []int64{0}
	for j := 1; j < n; j++ {
		dstEdges = append(dstEdges, int64(3*j+1))
	}
	dstEdges = append(dstEdges, int64(3*n))
	return xfer.MustMesh("bench-src", srcEdges...), xfer.MustMesh("bench-dst", dstEdges...)
}

func benchField(n int) []*big.Rat {
	f := xfer.ZeroField(n)
	for i := range f {
		f[i].SetInt64(int64(i%5) + 1)
	}
	return f
}

func BenchmarkBuildPlan(b *testing.B) {
	for _, n := range []int{256, 1024} {
		src, dst := benchMeshes(n)
		b.Run(sizeName(n), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				if _, err := xfer.BuildPlan(src, dst); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

// BenchmarkValidate measures plan validation on its own: the plan is built once
// outside the loop, so this is the cost of re-checking a plan that arrived from
// somewhere else.
func BenchmarkValidate(b *testing.B) {
	for _, n := range []int{256, 1024} {
		src, dst := benchMeshes(n)
		plan, err := xfer.BuildPlan(src, dst)
		if err != nil {
			b.Fatal(err)
		}
		b.Run(sizeName(n), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				if err := plan.Validate(); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

func BenchmarkApplyRows(b *testing.B) {
	for _, n := range []int{256, 1024} {
		src, dst := benchMeshes(n)
		plan, err := xfer.BuildPlan(src, dst)
		if err != nil {
			b.Fatal(err)
		}
		rho := benchField(n)
		dstVal := xfer.ZeroField(n)
		b.Run(sizeName(n), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				run := plan.NewRun()
				if _, err := run.ApplyRows(0, 1, src, dst, rho, func(j int, v *big.Rat) error {
					dstVal[j].Set(v)
					return nil
				}); err != nil {
					b.Fatal(err)
				}
				if err := run.Finish(); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

func BenchmarkGatherRows(b *testing.B) {
	for _, n := range []int{256, 1024} {
		src, dst := benchMeshes(n)
		plan, err := xfer.BuildPlan(src, dst)
		if err != nil {
			b.Fatal(err)
		}
		rho := benchField(n)
		dstVal := xfer.ZeroField(n)
		b.Run(sizeName(n), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				run := plan.NewRun()
				if _, err := run.GatherRows(0, 1, src, dst, rho, dstVal); err != nil {
					b.Fatal(err)
				}
				if err := run.Finish(); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

// BenchmarkGatherRowsW8 is the same work split over eight workers, each taking
// its turn in the sweep. It is the shape a task graph would actually produce.
func BenchmarkGatherRowsW8(b *testing.B) {
	const workers = 8
	n := 1024
	src, dst := benchMeshes(n)
	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		b.Fatal(err)
	}
	rho := benchField(n)
	dstVal := xfer.ZeroField(n)
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		run := plan.NewRun()
		for w := 0; w < workers; w++ {
			if _, err := run.GatherRows(w, workers, src, dst, rho, dstVal); err != nil {
				b.Fatal(err)
			}
		}
		if err := run.Finish(); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkApplyUnbound is the baseline for the identity gate: the same plan
// applied through the index-only control, which performs no mesh check.
func BenchmarkApplyUnbound(b *testing.B) {
	for _, n := range []int{256, 1024} {
		src, dst := benchMeshes(n)
		plan, err := xfer.BuildPlan(src, dst)
		if err != nil {
			b.Fatal(err)
		}
		rho := benchField(n)
		dstVal := xfer.ZeroField(n)
		b.Run(sizeName(n), func(b *testing.B) {
			b.ReportAllocs()
			for i := 0; i < b.N; i++ {
				if err := plan.ApplyUnbound(rho, dstVal, nil); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}

// BenchmarkRejectWrongMesh measures the identity gate itself: a plan applied to a
// destination mesh it is not bound to, refused before any row is computed.
func BenchmarkRejectWrongMesh(b *testing.B) {
	a := xfer.MustMesh("A", 0, 1, 3, 4)
	bb := xfer.MustMesh("B", 0, 2, 3, 4)
	c := xfer.MustMesh("C", 0, 1, 2, 4)
	plan, err := xfer.BuildPlan(a, bb)
	if err != nil {
		b.Fatal(err)
	}
	rho := benchField(a.Cells())
	dstVal := xfer.ZeroField(c.Cells())
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := plan.Apply(a, c, rho, dstVal); err == nil {
			b.Fatal("wrong mesh accepted")
		}
	}
}

// BenchmarkCheckIntegral is the cheap post-hoc conservation check.
func BenchmarkCheckIntegral(b *testing.B) {
	const n = 1024
	src, dst := benchMeshes(n)
	plan, err := xfer.BuildPlan(src, dst)
	if err != nil {
		b.Fatal(err)
	}
	rho := benchField(n)
	dstVal := xfer.ZeroField(n)
	if err := plan.Apply(src, dst, rho, dstVal); err != nil {
		b.Fatal(err)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if err := plan.CheckIntegral(rho, dstVal); err != nil {
			b.Fatal(err)
		}
	}
}

func sizeName(n int) string {
	switch n {
	case 256:
		return "cells256"
	case 1024:
		return "cells1024"
	}
	return "cells"
}
