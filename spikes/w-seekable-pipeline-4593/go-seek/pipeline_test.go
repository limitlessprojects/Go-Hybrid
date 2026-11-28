package seek_test

import (
	"errors"
	"testing"

	"github.com/limitlessprojects/spike-w4593-seek"
)

// Two chained 3-to-2 stages: lane 0 = a+c, lane 1 = b. The fixture from the
// issue: input 0..8 must give [10, 1, 11, 14].
func stage32() seek.Stage {
	return seek.Stage{
		Name: "3to2",
		In:   3,
		Out:  2,
		Deps: [][]int{{0, 2}, {1}},
		Eval: func(f []uint32) uint32 {
			if len(f) == 2 {
				return f[0] + f[1]
			}
			return f[0]
		},
	}
}

func fixture() seek.Pipeline {
	return seek.Pipeline{Stages: []seek.Stage{stage32(), stage32()}}
}

func compileFixture(t *testing.T) *seek.Compiled {
	t.Helper()
	c, err := seek.Compile(fixture())
	if err != nil {
		t.Fatalf("Compile: %v", err)
	}
	return c
}

func ramp(n int) []uint32 {
	out := make([]uint32, n)
	for i := range out {
		out[i] = uint32(i)
	}
	return out
}

func TestFixtureMatchesTheIssue(t *testing.T) {
	c := compileFixture(t)
	got, err := c.Dense(ramp(9))
	if err != nil {
		t.Fatalf("Dense: %v", err)
	}
	want := []uint32{10, 1, 11, 14}
	if len(got) != len(want) {
		t.Fatalf("len=%d want %d (%v)", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("dense[%d]=%d want %d (all=%v)", i, got[i], want[i], got)
		}
	}
	// The accessor must agree, coordinate by coordinate.
	for k := int64(0); k < int64(len(want)); k++ {
		v, err := c.At(seek.NewSnapshot(ramp(9)), k)
		if err != nil {
			t.Fatalf("At(%d): %v", k, err)
		}
		if v != want[k] {
			t.Fatalf("At(%d)=%d want %d", k, v, want[k])
		}
	}
}

func TestShortInputEmitsNothing(t *testing.T) {
	c := compileFixture(t)
	out, err := c.Dense(ramp(4))
	if err != nil {
		t.Fatalf("Dense: %v", err)
	}
	if len(out) != 0 {
		t.Fatalf("4 input words produced %d outputs, want 0", len(out))
	}
	s := seek.NewSnapshot(ramp(4))
	if _, err := c.At(s, 0); !errors.Is(err, seek.ErrRange) {
		t.Fatalf("At(0) on empty output: err=%v, want ErrRange", err)
	}
	if s.Reads() != 0 {
		t.Fatalf("invalid query touched %d source words, want 0", s.Reads())
	}
}

// TestSweep reproduces the issue's 10,584 in-range comparisons: lengths 0..128,
// three deterministic input patterns.
func TestSweep(t *testing.T) {
	c := compileFixture(t)
	patterns := []func(int) uint32{
		func(i int) uint32 { return uint32(i) },
		func(i int) uint32 { return uint32(7*i + 3) },
		func(i int) uint32 { return uint32((i*i)*2654435761 + 11) },
	}
	comparisons, invalid := 0, 0
	for n := 0; n <= 128; n++ {
		for _, pat := range patterns {
			words := make([]uint32, n)
			for i := range words {
				words[i] = pat(i)
			}
			dense, err := c.Dense(words)
			if err != nil {
				t.Fatalf("Dense(n=%d): %v", n, err)
			}
			for k := int64(0); k < int64(len(dense)); k++ {
				s := seek.NewSnapshot(words)
				v, err := c.At(s, k)
				if err != nil {
					t.Fatalf("At(n=%d,k=%d): %v", n, k, err)
				}
				if v != dense[k] {
					t.Fatalf("n=%d k=%d: At=%d dense=%d", n, k, v, dense[k])
				}
				if s.Reads() == 0 {
					t.Fatalf("n=%d k=%d read zero source words", n, k)
				}
				comparisons++
			}
			// Four out-of-range coordinates per (n, pattern), each of which
			// must read nothing at all.
			for _, bad := range []int64{-1, int64(len(dense)), int64(len(dense)) + 1, 1 << 40} {
				s := seek.NewSnapshot(words)
				if _, err := c.At(s, bad); !errors.Is(err, seek.ErrRange) {
					t.Fatalf("n=%d k=%d: err=%v, want ErrRange", n, bad, err)
				}
				if s.Reads() != 0 {
					t.Fatalf("n=%d k=%d read %d words", n, bad, s.Reads())
				}
				invalid++
			}
		}
	}
	t.Logf("comparisons=%d invalid_queries=%d", comparisons, invalid)
	if comparisons != 10584 {
		t.Fatalf("comparisons=%d, the issue reports 10584", comparisons)
	}
}

func TestLateSeekReadsExactlyTheDependencies(t *testing.T) {
	const n = 2_250_009
	words := make([]uint32, n)
	for i := range words {
		words[i] = uint32(i*3 + 1)
	}
	c := compileFixture(t)

	dense, err := c.Dense(words)
	if err != nil {
		t.Fatalf("Dense: %v", err)
	}
	if len(dense) <= 1_000_001 {
		t.Fatalf("pipeline produced %d outputs, need at least 1,000,002", len(dense))
	}

	cases := []struct {
		k    int64
		want []int
	}{
		{1_000_000, []int{2250000, 2250002, 2250003, 2250005}},
		{1_000_001, []int{2250001}},
	}
	for _, tc := range cases {
		s := seek.NewSnapshot(words)
		v, err := c.At(s, tc.k)
		if err != nil {
			t.Fatalf("At(%d): %v", tc.k, err)
		}
		if v != dense[tc.k] {
			t.Fatalf("At(%d)=%d dense=%d", tc.k, v, dense[tc.k])
		}
		got := s.Indices()
		if len(got) != len(tc.want) {
			t.Fatalf("k=%d read indices %v, want %v", tc.k, got, tc.want)
		}
		for i := range got {
			if got[i] != tc.want[i] {
				t.Fatalf("k=%d read indices %v, want %v", tc.k, got, tc.want)
			}
		}
	}
}

func TestSourceIndicesAgreesWithWhatAtTouches(t *testing.T) {
	c := compileFixture(t)
	words := ramp(300)
	for k := int64(0); k < 20; k++ {
		s := seek.NewSnapshot(words)
		if _, err := c.At(s, k); err != nil {
			t.Fatalf("At(%d): %v", k, err)
		}
		plan, err := c.SourceIndices(k, int64(len(words)))
		if err != nil {
			t.Fatalf("SourceIndices(%d): %v", k, err)
		}
		got := s.Indices()
		if len(got) != len(plan) {
			t.Fatalf("k=%d touched %v, plan %v", k, got, plan)
		}
		for i := range got {
			if got[i] != plan[i] {
				t.Fatalf("k=%d touched %v, plan %v", k, got, plan)
			}
		}
	}
}

func TestStatefulStageIsRefused(t *testing.T) {
	p := seek.Pipeline{Stages: []seek.Stage{{
		Name:     "running-sum",
		In:       1,
		Out:      1,
		Deps:     [][]int{{0}},
		Stateful: true,
		Eval:     func(f []uint32) uint32 { return f[0] },
	}}}
	if _, err := seek.Compile(p); !errors.Is(err, seek.ErrStateful) {
		t.Fatalf("err=%v, want ErrStateful", err)
	}
}

func TestBadDependencyIsRefused(t *testing.T) {
	p := seek.Pipeline{Stages: []seek.Stage{{
		Name: "bogus", In: 2, Out: 1,
		Deps: [][]int{{0, 5}},
		Eval: func(f []uint32) uint32 { return f[0] },
	}}}
	if _, err := seek.Compile(p); !errors.Is(err, seek.ErrDep) {
		t.Fatalf("err=%v, want ErrDep", err)
	}
}

func TestCoordinateOverflowIsRefused(t *testing.T) {
	// A 1-to-3 stage triples the coordinate space on every pass; two of them
	// overflow int64 for a source length of 2^62.
	p := seek.Pipeline{Stages: []seek.Stage{
		{Name: "dup3", In: 1, Out: 3, Deps: [][]int{{0}, {0}, {0}}, Eval: func(f []uint32) uint32 { return f[0] }},
		{Name: "dup3", In: 1, Out: 3, Deps: [][]int{{0}, {0}, {0}}, Eval: func(f []uint32) uint32 { return f[0] }},
	}}
	c, err := seek.Compile(p)
	if err != nil {
		t.Fatalf("Compile: %v", err)
	}
	if _, err := c.OutLen(1 << 62); !errors.Is(err, seek.ErrOverflow) {
		t.Fatalf("err=%v, want ErrOverflow", err)
	}
}

// TestPlanLengthComesFromTheSnapshot: the accessor derives the output length
// from the snapshot it is handed, so a plan can never be applied to a shorter
// source behind its back. A short source simply has fewer outputs.
func TestPlanLengthComesFromTheSnapshot(t *testing.T) {
	c := compileFixture(t)
	outLen, err := c.OutLen(3) // 3 words -> 2 -> 0 outputs
	if err != nil || outLen != 0 {
		t.Fatalf("OutLen(3)=%d,%v want 0", outLen, err)
	}
	s := seek.NewSnapshot(ramp(3))
	if _, err := c.At(s, 0); !errors.Is(err, seek.ErrRange) {
		t.Fatalf("err=%v, want ErrRange", err)
	}
	if s.Reads() != 0 {
		t.Fatalf("read %d words on an empty plan", s.Reads())
	}
}

// TestThreeStagePipeline checks the composition is not hard-coded to two
// stages: 3->2, 3->2, then 2->1 (a+b).
func TestThreeStagePipeline(t *testing.T) {
	p := seek.Pipeline{Stages: []seek.Stage{
		stage32(),
		stage32(),
		{Name: "2to1", In: 2, Out: 1, Deps: [][]int{{0, 1}}, Eval: func(f []uint32) uint32 { return f[0] ^ f[1] }},
	}}
	c, err := seek.Compile(p)
	if err != nil {
		t.Fatalf("Compile: %v", err)
	}
	words := ramp(300)
	dense, err := c.Dense(words)
	if err != nil {
		t.Fatalf("Dense: %v", err)
	}
	if len(dense) == 0 {
		t.Fatalf("no outputs")
	}
	for k := int64(0); k < int64(len(dense)); k++ {
		s := seek.NewSnapshot(words)
		v, err := c.At(s, k)
		if err != nil {
			t.Fatalf("At(%d): %v", k, err)
		}
		if v != dense[k] {
			t.Fatalf("At(%d)=%d dense=%d", k, v, dense[k])
		}
	}
}
