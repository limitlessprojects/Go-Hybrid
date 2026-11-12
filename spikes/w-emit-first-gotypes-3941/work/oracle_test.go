package oracle

import (
	"go/types"
	"testing"
)

// TestP1Stage1TypeProbeIntrinsics verifies Falsifier P1: Stage-1 generic probe
// intrinsics resolve map-slot loans, channel receives, try-propagation tuples,
// and uncurated stdlib methods (e.g. flag.FlagSet.BoolFunc from #4252) without shape.rs.
func TestP1Stage1TypeProbeIntrinsics(t *testing.T) {
	imp := NewCachedImporter()
	src := `package main

import "flag"

func __hy_slot_loan[K comparable, V any](m map[K]V, k K) *V {
	var zero V
	_ = m[k]
	return &zero
}

func __hy_try1[T any](v T, err error) T {
	_ = err
	return v
}

func probe(fs *flag.FlagSet, ch <-chan []int) {
	m := map[string]int{"ok": 1}
	slot := __hy_slot_loan(m, "ok")
	recvVal := <-ch
	fs.BoolFunc("v", "verbose", func(string) error { return nil })
	_, _ = slot, recvVal
}
`
	res, err := CheckBuffer(src, nil, imp)
	if err != nil {
		t.Fatalf("parse failed: %v", err)
	}
	if len(res.Diags) != 0 {
		t.Fatalf("expected 0 diagnostics on valid Stage-1 probe, got: %+v", res.Diags)
	}
	if !res.VarFacts["slot"].MayShare {
		t.Fatalf("expected slot (*int) MayShare=true, got %+v", res.VarFacts["slot"])
	}
	if !res.VarFacts["recvVal"].MayShare {
		t.Fatalf("expected recvVal ([]int) MayShare=true, got %+v", res.VarFacts["recvVal"])
	}
}

// TestP2GenericInstantiationAndSpanMapping verifies Falsifier P2:
// (a) nested generic struct instantiations Box[[]map[string]int] (#3948) and
//     Pair[int, func()] (#3939) classify MayShare/HoldsClosure with zero custom unifiers,
// (b) go/types errors map back to exact .hy spans and HYB-E1002 codes.
func TestP2GenericInstantiationAndSpanMapping(t *testing.T) {
	imp := NewCachedImporter()
	src := `package main

type Box[T any] struct { Items T }
type Pair[A any, B any] struct { First A; Second B }

func check() {
	bScalar := Box[int]{Items: 42}
	bNested := Box[[]map[string]int]{Items: nil}
	pClos := Pair[int, func()]{First: 1, Second: func() {}}
	fnLocal := func(x int) int { return x }
	bad := fnLocal("wrong")
	_, _, _, _ = bScalar, bNested, pClos, bad
}
`
	lineMap := map[int]HySpan{
		11: {File: "closure_arg.hy", Line: 7, Col: 16, NodeID: 44, HintCode: "HYB-E1002"},
	}
	res, err := CheckBuffer(src, lineMap, imp)
	if err != nil {
		t.Fatalf("parse failed: %v", err)
	}
	if res.VarFacts["bScalar"].MayShare || res.VarFacts["bScalar"].HoldsClosure || !res.VarFacts["bScalar"].IsSendable {
		t.Fatalf("bScalar Box[int] should be pure scalar sendable: %+v", res.VarFacts["bScalar"])
	}
	if !res.VarFacts["bNested"].MayShare || res.VarFacts["bNested"].IsSendable {
		t.Fatalf("bNested Box[[]map[string]int] must have MayShare=true: %+v", res.VarFacts["bNested"])
	}
	if !res.VarFacts["pClos"].HoldsClosure || res.VarFacts["pClos"].IsSendable {
		t.Fatalf("pClos Pair[int, func()] must have HoldsClosure=true: %+v", res.VarFacts["pClos"])
	}
	if len(res.Diags) != 1 {
		t.Fatalf("expected 1 mapped diagnostic, got %+v", res.Diags)
	}
	d := res.Diags[0]
	if d.Code != "HYB-E1002" || d.HyFile != "closure_arg.hy" || d.HyLine != 7 || d.HyCol != 16 {
		t.Fatalf("unexpected span mapping: %+v", d)
	}
}

// TestP3ForeignSignatureWall verifies Falsifier P3's key boundary discovery:
// go/types.Signature alone cannot distinguish read-only slice inspection
// (bytes.Equal([]byte, []byte)) from in-place slice mutation (sort.Ints([]int))
// because Go lacks `readonly []T` in types.Signature.
func TestP3ForeignSignatureWall(t *testing.T) {
	imp := NewCachedImporter()
	bytesPkg, err := imp.Import("bytes")
	if err != nil {
		t.Fatalf("import bytes: %v", err)
	}
	sortPkg, err := imp.Import("sort")
	if err != nil {
		t.Fatalf("import sort: %v", err)
	}
	eqSig := bytesPkg.Scope().Lookup("Equal").Type().(*types.Signature)
	intsSig := sortPkg.Scope().Lookup("Ints").Type().(*types.Signature)

	// Both take a slice as parameter 0, so a pure types.Signature rule marks both mutable:
	if !SignatureParamMutates(eqSig.Params().At(0).Type()) {
		t.Fatal("expected pure signature check to see []byte on bytes.Equal")
	}
	if !SignatureParamMutates(intsSig.Params().At(0).Type()) {
		t.Fatal("expected pure signature check to see []int on sort.Ints")
	}
}

// BenchmarkP4WarmCachedTypecheck measures warm per-file check + fact extraction
// with a resident CachedImporter (Falsifier P4 target: < 5.0 ms/op).
func BenchmarkP4WarmCachedTypecheck(b *testing.B) {
	imp := NewCachedImporter()
	src := `package main

import (
	"bytes"
	"context"
	"flag"
	"sort"
)

type Box[T any] struct { Val T }

func run(ctx context.Context, fs *flag.FlagSet) bool {
	_ = ctx.Err()
	fs.BoolFunc("v", "verbose", nil)
	xs := []int{3, 1, 2}
	sort.Ints(xs)
	box := Box[[]int]{Val: xs}
	return len(box.Val) == 3 && bytes.Equal([]byte("a"), []byte("a"))
}
`
	// Prime stdlib importer cache once (resident worker startup).
	if _, err := CheckBuffer(src, nil, imp); err != nil {
		b.Fatal(err)
	}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := CheckBuffer(src, nil, imp)
		if err != nil {
			b.Fatal(err)
		}
	}
}
