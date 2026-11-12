package w3900progressstrategies

import (
	"testing"
)

// ---------------------------------------------------------------------------
// TestAllTwentyRegions: runs the full 20-region pipeline and logs summary
// ---------------------------------------------------------------------------

func TestAllTwentyRegionsVerdicts(t *testing.T) {
	results := VerifyAll(DefaultKMin)

	if len(results) != 20 {
		t.Fatalf("expected 20 region results, got %d", len(results))
	}

	admitCount := 0
	rejectCount := 0
	unknownCount := 0
	for _, r := range results {
		switch r.Verdict {
		case VerdictADMIT:
			admitCount++
		case VerdictREJECT:
			rejectCount++
		case VerdictUNKNOWN:
			unknownCount++
		}
	}

	t.Logf("Verdict summary: ADMIT=%d REJECT=%d UNKNOWN=%d (total=%d)",
		admitCount, rejectCount, unknownCount, len(results))

	// Every region must produce a well-formed result
	for _, r := range results {
		if r.Region.ID == "" {
			t.Error("region ID must not be empty")
		}
		switch r.Verdict {
		case VerdictADMIT, VerdictREJECT, VerdictUNKNOWN:
		default:
			t.Errorf("invalid verdict %v for region %s", r.Verdict, r.Region.ID)
		}
		if r.StatesSeen > MaxStates {
			t.Errorf("region %s: states seen %d exceeds cap %d", r.Region.ID, r.StatesSeen, MaxStates)
		}
		if r.EdgesTraversed > MaxEdges {
			t.Errorf("region %s: edges traversed %d exceeds cap %d", r.Region.ID, r.EdgesTraversed, MaxEdges)
		}
	}
}

// ---------------------------------------------------------------------------
// F1 falsifier unit tests
// ---------------------------------------------------------------------------

func TestF1KminBelowOneIsCounterexample(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges:    []Edge{{From: 0, To: 1, Cost: 1}},
		Terminal: []State{1},
		Rank:     map[State]int{0: 100, 1: 50},
	}
	ar := AttractorResult{Found: true, Attractor: []State{0, 1}}

	if !F1Counterexample(g, ar.Attractor, 1.0) {
		t.Error("F1: k_min <= 1 should be a counterexample")
	}
}

func TestF1SerialGraphBelowKmin(t *testing.T) {
	g := &Game{
		States:   []State{0, 1, 2},
		Edges:    []Edge{{From: 0, To: 1, Cost: 1}, {From: 1, To: 2, Cost: 1}},
		Terminal: []State{2},
		Rank:     map[State]int{0: 100, 1: 50, 2: 0},
	}
	ar := AttractorResult{Found: true, Attractor: []State{0, 1, 2}}

	if !F1Counterexample(g, ar.Attractor, 1.5) {
		t.Error("F1: serial graph (ratio=0) should be counterexample for k_min=1.5")
	}
}

func TestF1ConcurrentGraphAboveKmin(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges: []Edge{
			{From: 0, To: 1, Cost: 1},
			{From: 0, To: 1, Cost: 2},
			{From: 0, To: 1, Cost: 3},
		},
		Terminal: []State{1},
		Rank:     map[State]int{0: 100, 1: 50},
	}
	ar := AttractorResult{Found: true, Attractor: []State{0, 1}}

	if F1Counterexample(g, ar.Attractor, 1.5) {
		t.Error("F1: concurrent graph (ratio=3) should NOT be counterexample for k_min=1.5")
	}
}

// ---------------------------------------------------------------------------
// F2 falsifier unit tests (M1/M2 preemption boundary)
// ---------------------------------------------------------------------------

func TestF2BranchingViolatesPreemptionFree(t *testing.T) {
	// State 0 has two outgoing edges within the attractor -> M1 preemption-free violated
	g := &Game{
		States:   []State{0, 1, 2},
		Edges: []Edge{
			{From: 0, To: 1, Cost: 1},
			{From: 0, To: 2, Cost: 1},
		},
		Terminal: []State{1, 2},
		Rank:     map[State]int{0: 100, 1: 50, 2: 50},
	}
	ar := AttractorResult{Found: true, Attractor: []State{0, 1, 2}}

	if !F2Counterexample(g, ar.Attractor) {
		t.Error("F2: branching state in attractor should violate M1 preemption-free")
	}
}

func TestF2SingleEdgePerStateOK(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges:    []Edge{{From: 0, To: 1, Cost: 1}},
		Terminal: []State{1},
		Rank:     map[State]int{0: 100, 1: 50},
	}
	ar := AttractorResult{Found: true, Attractor: []State{0, 1}}

	if F2Counterexample(g, ar.Attractor) {
		t.Error("F2: single-edge graph should not violate M1 preemption-free")
	}
}

// ---------------------------------------------------------------------------
// F3 falsifier unit tests (runtime witness to losing sink)
// ---------------------------------------------------------------------------

func TestF3LosingSinkReachable(t *testing.T) {
	g := &Game{
		States:   []State{0, 1, 2},
		Edges:    []Edge{{From: 0, To: 1, Cost: 1}, {From: 1, To: 2, Cost: 1}},
		Start:    0,
		Terminal: []State{1},
		LosingSink: 2,
		Rank:     map[State]int{0: 100, 1: 50, 2: 0},
	}

	if !F3Counterexample(g) {
		t.Error("F3: losing sink should be reachable from start")
	}
}

func TestF3LosingSinkUnreachable(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges:    []Edge{{From: 0, To: 1, Cost: 1}},
		Start:    0,
		Terminal: []State{1},
		LosingSink: 99,
		Rank:     map[State]int{0: 100, 1: 50},
	}

	if F3Counterexample(g) {
		t.Error("F3: losing sink not in graph should be unreachable")
	}
}

// ---------------------------------------------------------------------------
// Rank certificate validation unit tests
// ---------------------------------------------------------------------------

func TestValidateRankStrictlyDecreasing(t *testing.T) {
	g := &Game{
		States:   []State{0, 1, 2},
		Edges: []Edge{
			{From: 0, To: 1, Cost: 1},
			{From: 1, To: 2, Cost: 2},
		},
		Terminal: []State{2},
		Rank:     map[State]int{0: 100, 1: 50, 2: 10},
	}

	if !ValidateRankCertificate(g, []State{0, 1, 2}) {
		t.Error("rank should be strictly decreasing: 100 > 50 > 10")
	}
}

func TestValidateRankNotDecreasing(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges:    []Edge{{From: 0, To: 1, Cost: 1}},
		Terminal: []State{1},
		Rank:     map[State]int{0: 10, 1: 10},
	}

	if ValidateRankCertificate(g, []State{0, 1}) {
		t.Error("rank should NOT be strictly decreasing when equal")
	}
}

func TestValidateRankInsufficientDecrease(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges:    []Edge{{From: 0, To: 1, Cost: 10}},
		Terminal: []State{1},
		Rank:     map[State]int{0: 100, 1: 95},
	}

	if ValidateRankCertificate(g, []State{0, 1}) {
		t.Error("rank decrease (5) should be insufficient for edge cost (10)")
	}
}

// ---------------------------------------------------------------------------
// Channel-close validation unit tests
// ---------------------------------------------------------------------------

func TestCheckChannelCloseNoSendAfterClose(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges:    []Edge{{From: 0, To: 1, Cost: 1}},
		Terminal: []State{1},
		ChannelCloses: []ChannelClose{
			{State: 1, Closed: true},
		},
		Rank: map[State]int{0: 100, 1: 50},
	}

	if !CheckChannelClose(g, []State{0, 1}) {
		t.Error("channel close at terminal state should be OK")
	}
}

func TestCheckChannelCloseSendAfterClose(t *testing.T) {
	g := &Game{
		States:   []State{0, 1, 2},
		Edges: []Edge{
			{From: 0, To: 1, Cost: 1},
			{From: 1, To: 2, Cost: 1},
		},
		Terminal: []State{2},
		ChannelCloses: []ChannelClose{
			{State: 1, Closed: true},
		},
		Rank: map[State]int{0: 100, 1: 50, 2: 0},
	}

	if CheckChannelClose(g, []State{0, 1, 2}) {
		t.Error("channel close at non-terminal state with outgoing edge should fail")
	}
}

// ---------------------------------------------------------------------------
// ParallelismRatio unit tests
// ---------------------------------------------------------------------------

func TestParallelismRatioAllSerial(t *testing.T) {
	g := &Game{
		States:   []State{0, 1, 2},
		Edges: []Edge{
			{From: 0, To: 1, Cost: 1},
			{From: 1, To: 2, Cost: 1},
		},
		Terminal: []State{2},
		Rank:     map[State]int{0: 100, 1: 50, 2: 0},
	}

	ratio := ParallelismRatio(g, []State{0, 1, 2})
	if ratio != 0.0 {
		t.Errorf("all-serial graph ratio should be 0, got %f", ratio)
	}
}

func TestParallelismRatioAllConcurrent(t *testing.T) {
	g := &Game{
		States:   []State{0, 1},
		Edges: []Edge{
			{From: 0, To: 1, Cost: 1},
			{From: 0, To: 1, Cost: 2},
			{From: 0, To: 1, Cost: 3},
		},
		Terminal: []State{1},
		Rank:     map[State]int{0: 100, 1: 50},
	}

	ratio := ParallelismRatio(g, []State{0, 1})
	if ratio < 1.0 {
		t.Errorf("all-concurrent graph ratio should be >= 1, got %f", ratio)
	}
}

// ---------------------------------------------------------------------------
// VerifyRegion determinism
// ---------------------------------------------------------------------------

func TestVerifyRegionDeterministic(t *testing.T) {
	r := Region{ID: "R05", Seed: 0x05, Valid: true}
	res1 := VerifyRegion(r, DefaultKMin)
	res2 := VerifyRegion(r, DefaultKMin)

	if res1.Verdict != res2.Verdict {
		t.Error("VerifyRegion should be deterministic")
	}
	if res1.StatesSeen != res2.StatesSeen {
		t.Error("VerifyRegion states seen should be deterministic")
	}
}

// ---------------------------------------------------------------------------
// Budget caps
// ---------------------------------------------------------------------------

func TestBudgetCapsArePositive(t *testing.T) {
	if MaxStates <= 0 || MaxEdges <= 0 || MaxCPUSecs <= 0 {
		t.Error("budget caps must be positive")
	}
}

// ---------------------------------------------------------------------------
// Verdict string
// ---------------------------------------------------------------------------

func TestVerdictString(t *testing.T) {
	if VerdictADMIT.String() != "ADMIT" {
		t.Error("ADMIT string")
	}
	if VerdictREJECT.String() != "REJECT" {
		t.Error("REJECT string")
	}
	if VerdictUNKNOWN.String() != "UNKNOWN" {
		t.Error("UNKNOWN string")
	}
}

// ---------------------------------------------------------------------------
// TestPreregConformance: pre-registered 20-region conformance (V1-e)
// ---------------------------------------------------------------------------

// expectedVerdict maps a pre-registered region ID to its mandated verdict
// class per PREREG.md §1.
func expectedVerdict(id string) Verdict {
	switch {
	case id >= "R01" && id <= "R10":
		return VerdictADMIT
	case (id >= "N01" && id <= "N05") || (id >= "N08" && id <= "N10"):
		return VerdictREJECT
	case id == "N06" || id == "N07":
		return VerdictUNKNOWN
	default:
		return Verdict(-1)
	}
}

func TestPreregConformance(t *testing.T) {
	admitCount := 0
	rejectCount := 0
	unknownCount := 0

	for _, r := range AllRegions {
		res := VerifyRegion(r, DefaultKMin)

		want := expectedVerdict(r.ID)
		if want == Verdict(-1) {
			t.Fatalf("unknown region id %s", r.ID)
		}
		if res.Verdict != want {
			t.Errorf("%s: expected %v, got %v", r.ID, want, res.Verdict)
		}
		switch res.Verdict {
		case VerdictADMIT:
			admitCount++
		case VerdictREJECT:
			rejectCount++
		case VerdictUNKNOWN:
			unknownCount++
		}

		// Budget caps must never be exceeded.
		if res.StatesSeen > MaxStates {
			t.Errorf("%s: states seen %d exceeds cap %d", r.ID, res.StatesSeen, MaxStates)
		}
		if res.EdgesTraversed > MaxEdges {
			t.Errorf("%s: edges traversed %d exceeds cap %d", r.ID, res.EdgesTraversed, MaxEdges)
		}
	}

	if admitCount != 10 {
		t.Errorf("expected 10 ADMIT, got %d", admitCount)
	}
	if rejectCount != 8 {
		t.Errorf("expected 8 REJECT, got %d", rejectCount)
	}
	if unknownCount != 2 {
		t.Errorf("expected 2 UNKNOWN, got %d", unknownCount)
	}

	// R06 (seed 0x06) must land exactly on the k_min boundary: ratio == 1.5.
	r06 := Region{ID: "R06", Seed: 0x06, Valid: true}
	if res := VerifyRegion(r06, DefaultKMin); res.Verdict != VerdictADMIT {
		t.Errorf("R06: expected ADMIT, got %v", res.Verdict)
	} else if res.ParallelismRatio != 1.5 {
		t.Errorf("R06: ParallelismRatio = %f, want exactly 1.5", res.ParallelismRatio)
	}
}

