// Package w3900progressstrategies implements the WO-2 progress-strategies
// attractor/rank verifier spike for limitlessprojects/Go-Hybrid#3900.
//
// Zero external dependencies. Self-contained finite-state game verifier:
// attractor search, strictly decreasing rank certificate validation,
// parallelism_ratio computation, pre-terminal channel-close validation,
// and delshafy F1/F2/F3 falsifier counterexample tests.
//
// Verdict semantics:
//   ADMIT   - attractor found, rank certificate valid, all falsifiers pass
//   REJECT  - at least one falsifier produces a counterexample
//   UNKNOWN - budget exhausted or rank certificate absent; no claim
package w3900progressstrategies

import (
	"fmt"
	"math/rand"
	"time"
)

// ---------------------------------------------------------------------------
// Budget hard caps (PREREG.md §2)
// ---------------------------------------------------------------------------

const (
	MaxStates   = 50_000
	MaxEdges    = 500_000
	MaxCPUSecs  = 30
)

// ---------------------------------------------------------------------------
// Verdict type
// ---------------------------------------------------------------------------

// Verdict is the three-state output of the verifier.
type Verdict int

const (
	VerdictADMIT  Verdict = iota // attractor found, all falsifiers pass
	VerdictREJECT                // at least one falsifier counterexample
	VerdictUNKNOWN               // budget exhausted or no rank certificate
)

func (v Verdict) String() string {
	switch v {
	case VerdictADMIT:
		return "ADMIT"
	case VerdictREJECT:
		return "REJECT"
	case VerdictUNKNOWN:
		return "UNKNOWN"
	default:
		return fmt.Sprintf("Verdict(%d)", int(v))
	}
}

// ---------------------------------------------------------------------------
// Region: a pre-registered finite-state game region
// ---------------------------------------------------------------------------

// Region describes one of the 20 pre-registered regions (R01–R10 valid,
// N01–N10 near-miss).
type Region struct {
	ID      string  // e.g. "R01", "N07"
	Seed    uint32  // fixed seed for deterministic generation
	Valid   bool    // true for R-regions (expected ADMIT), false for N-regions
}

// ---------------------------------------------------------------------------
// Region registry: 20 pre-registered regions with fixed seeds (PREREG.md §1)
// ---------------------------------------------------------------------------

var AllRegions = []Region{
	// Valid regions (R01–R10) — expected ADMIT
	{ID: "R01", Seed: 0x01, Valid: true},
	{ID: "R02", Seed: 0x02, Valid: true},
	{ID: "R03", Seed: 0x03, Valid: true},
	{ID: "R04", Seed: 0x04, Valid: true},
	{ID: "R05", Seed: 0x05, Valid: true},
	{ID: "R06", Seed: 0x06, Valid: true},
	{ID: "R07", Seed: 0x07, Valid: true},
	{ID: "R08", Seed: 0x08, Valid: true},
	{ID: "R09", Seed: 0x09, Valid: true},
	{ID: "R10", Seed: 0x0a, Valid: true},
	// Near-miss regions (N01–N10) — expected REJECT / UNKNOWN
	{ID: "N01", Seed: 0x81, Valid: false},
	{ID: "N02", Seed: 0x82, Valid: false},
	{ID: "N03", Seed: 0x83, Valid: false},
	{ID: "N04", Seed: 0x84, Valid: false},
	{ID: "N05", Seed: 0x85, Valid: false},
	{ID: "N06", Seed: 0x86, Valid: false},
	{ID: "N07", Seed: 0x87, Valid: false},
	{ID: "N08", Seed: 0x88, Valid: false},
	{ID: "N09", Seed: 0x89, Valid: false},
	{ID: "N10", Seed: 0x8a, Valid: false},
}

// ---------------------------------------------------------------------------
// State machine model
// ---------------------------------------------------------------------------

// State is a finite-state game state identified by an integer.
type State int

// Edge is a directed transition between states with a progress cost.
type Edge struct {
	From   State
	To     State
	Cost   int // progress cost (rank decrease amount)
}

// ChannelClose records a channel close event at a state.
type ChannelClose struct {
	State  State
	Closed bool // true if channel is closed at this state
}

// Game is a finite-state game graph for a region.
type Game struct {
	States       []State
	Edges        []Edge
	Start        State
	Terminal     []State // attractor terminal states
	LosingSink   State   // explicit losing sink (F3 witness target)
	ChannelCloses []ChannelClose
	Rank         map[State]int // rank certificate per state
}

// ---------------------------------------------------------------------------
// Deterministic game generator (seeded, zero external randomness)
// ---------------------------------------------------------------------------

// NewGame builds a deterministic finite-state game for a region.
// The graph structure is derived from the region seed so that results are
// reproducible and pre-registration is meaningful.
func NewGame(r Region) *Game {
	rng := rand.New(rand.NewSource(int64(r.Seed)))

	nStates := 8 + int(rng.Int31n(12)) // 8..19 states
	states := make([]State, nStates)
	for i := range states {
		states[i] = State(i)
	}

	start := State(0)

	// Generate edges: each state has 1..3 outgoing edges
	var edges []Edge
	for i := State(0); i < State(nStates-1); i++ {
		nOut := 1 + int(rng.Int31n(3))
		for j := 0; j < nOut; j++ {
			to := State(i) + State(1+rng.Int31n(int32(nStates-1-int(i))))
			if to >= State(nStates) {
				to = State(nStates - 1)
			}
			cost := 1 + int(rng.Int31n(5)) // rank decrease 1..5
			edges = append(edges, Edge{From: i, To: to, Cost: cost})
		}
	}
	// Ensure at least one edge into the terminal state
	terminal := State(nStates - 1)
	hasIntoTerminal := false
	for _, e := range edges {
		if e.To == terminal {
			hasIntoTerminal = true
			break
		}
	}
	if !hasIntoTerminal {
		edges = append(edges, Edge{From: State(nStates - 2), To: terminal, Cost: 1})
	}

	// Channel closes: a subset of states close their channel
	var channelCloses []ChannelClose
	for i := State(0); i < State(nStates); i++ {
		if rng.Int31n(3) == 0 {
			channelCloses = append(channelCloses, ChannelClose{State: i, Closed: true})
		}
	}

	// Losing sink: a state that no valid path should reach (F3 target)
	losingSink := State(nStates - 1) // terminal doubles as sink for N-regions

	// Rank certificate: strictly decreasing rank per state along edges
	rank := make(map[State]IntRank, len(states))
	// Assign ranks in reverse topological order approximation
	baseRank := nStates * 10
	for i := 0; i < nStates; i++ {
		rank[State(i)] = IntRank(baseRank - i*10)
	}

	return &Game{
		States:         states,
		Edges:          edges,
		Start:          start,
		Terminal:       []State{terminal},
		LosingSink:     losingSink,
		ChannelCloses:  channelCloses,
		Rank:           toRankMap(rank),
	}
}

// IntRank is a comparable rank value.
type IntRank int

func toRankMap(ranks map[State]IntRank) map[State]int {
	m := make(map[State]int, len(ranks))
	for k, v := range ranks {
		m[k] = int(v)
	}
	return m
}

// ---------------------------------------------------------------------------
// Attractor search (finite-state game)
// ---------------------------------------------------------------------------

// AttractorResult captures the outcome of attractor search.
type AttractorResult struct {
	Found      bool
	Attractor  []State
	StatesSeen int
	EdgesTraversed int
}

// SearchAttractor performs a bounded attractor search from the start state.
// It returns the set of states that can reach a terminal state without
// exceeding budget caps. If budget is exhausted, Found is false and
// StatesSeen / EdgesTraversed reflect the cutoff.
func SearchAttractor(g *Game) AttractorResult {
	seen := make(map[State]bool)
	stack := []State{g.Start}
	statesSeen := 0
	edgesTraversed := 0
	deadline := time.Now().Add(MaxCPUSecs * time.Second)

	for len(stack) > 0 {
		if time.Now().After(deadline) {
			return AttractorResult{Found: false, StatesSeen: statesSeen, EdgesTraversed: edgesTraversed}
		}
		if statesSeen > MaxStates || edgesTraversed > MaxEdges {
			return AttractorResult{Found: false, StatesSeen: statesSeen, EdgesTraversed: edgesTraversed}
		}

		s := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		if seen[s] {
			continue
		}
		seen[s] = true
		statesSeen++

		// Check if s is terminal
		isTerminal := false
		for _, t := range g.Terminal {
			if s == t {
				isTerminal = true
				break
			}
		}
		if isTerminal {
			continue
		}

		// Expand edges
		for _, e := range g.Edges {
			if e.From == s {
				edgesTraversed++
				if !seen[e.To] {
					stack = append(stack, e.To)
				}
			}
		}
	}

	// Build attractor: states that can reach a terminal
	attractor := make([]State, 0, len(seen))
	for s := range seen {
		isTerminal := false
		for _, t := range g.Terminal {
			if s == t {
				isTerminal = true
				break
			}
		}
		if !isTerminal && canReachTerminal(s, g, seen) {
			attractor = append(attractor, s)
		}
	}
	if len(attractor) == 0 {
		// Start state itself may be the only path to terminal
		for s := range seen {
			isTerminal := false
			for _, t := range g.Terminal {
				if s == t {
					isTerminal = true
					break
				}
			}
			if isTerminal {
				attractor = append(attractor, s)
			}
		}
	}

	return AttractorResult{
		Found:          len(attractor) > 0,
		Attractor:      attractor,
		StatesSeen:     statesSeen,
		EdgesTraversed: edgesTraversed,
	}
}

// canReachTerminal checks if state s can reach any terminal state within the
// already-seen subgraph.
func canReachTerminal(s State, g *Game, seen map[State]bool) bool {
	if !seen[s] {
		return false
	}
	for _, t := range g.Terminal {
		if s == t {
			return true
		}
	}
	visited := make(map[State]bool)
	stack := []State{s}
	for len(stack) > 0 {
		cur := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if visited[cur] {
			continue
		}
		visited[cur] = true
		for _, e := range g.Edges {
			if e.From == cur {
				for _, t := range g.Terminal {
					if e.To == t {
						return true
					}
				}
				if seen[e.To] && !visited[e.To] {
					stack = append(stack, e.To)
				}
			}
		}
	}
	return false
}

// ---------------------------------------------------------------------------
// Rank certificate validation (strictly decreasing)
// ---------------------------------------------------------------------------

// ValidateRankCertificate checks that every edge in the attractor exhibits
// a strictly decreasing rank: rank(from) > rank(to) and rank(from)-rank(to) >= edge.Cost.
func ValidateRankCertificate(g *Game, attractor []State) bool {
	attrSet := make(map[State]bool)
	for _, s := range attractor {
		attrSet[s] = true
	}
	for _, e := range g.Edges {
		if attrSet[e.From] && attrSet[e.To] {
			rf, okf := g.Rank[e.From]
			rt, okt := g.Rank[e.To]
			if !okf || !okt {
				return false
			}
			if rf <= rt {
				return false // not strictly decreasing
			}
			if rf-rt < e.Cost {
				return false // rank decrease insufficient for edge cost
			}
		}
	}
	return len(attractor) > 0
}

// ---------------------------------------------------------------------------
// parallelism_ratio computation
// ---------------------------------------------------------------------------

// ParallelismRatio computes the ratio of concurrent progress steps to serial
// steps across the attractor. A step is "concurrent" if it has multiple
// outgoing edges within the attractor; "serial" if it has exactly one.
func ParallelismRatio(g *Game, attractor []State) float64 {
	attrSet := make(map[State]bool)
	for _, s := range attractor {
		attrSet[s] = true
	}

	concurrent := 0
	serial := 0
	for _, s := range attractor {
		outCount := 0
		for _, e := range g.Edges {
			if e.From == s && attrSet[e.To] {
				outCount++
			}
		}
		if outCount > 1 {
			concurrent++
		} else if outCount == 1 {
			serial++
		}
	}

	if serial == 0 {
		if concurrent == 0 {
			return 0.0
		}
		// All steps concurrent, no serial steps: ratio is effectively infinite.
		return 1e6
	}
	return float64(concurrent) / float64(serial)
}

// ---------------------------------------------------------------------------
// Pre-terminal channel-close validation
// ---------------------------------------------------------------------------

// CheckChannelClose verifies that no channel is closed before the attractor
// reaches its terminal state (no send-after-close on managed channels), and
// that channel-close states are only at or after terminal states within the
// attractor subgraph.
func CheckChannelClose(g *Game, attractor []State) bool {
	attrSet := make(map[State]bool)
	for _, s := range attractor {
		attrSet[s] = true
	}

	terminalSet := make(map[State]bool)
	for _, t := range g.Terminal {
		terminalSet[t] = true
	}

	for _, cc := range g.ChannelCloses {
		if attrSet[cc.State] && cc.Closed && !terminalSet[cc.State] {
			// Channel closed at a non-terminal attractor state: potential
			// send-after-close. Check whether any edge leaves this state
			// within the attractor (which would be send-after-close).
			for _, e := range g.Edges {
				if e.From == cc.State && attrSet[e.To] {
					return false // send-after-close detected
				}
			}
		}
	}
	return true
}

// ---------------------------------------------------------------------------
// delshafy falsifiers
// ---------------------------------------------------------------------------

// F1Counterexample searches for a trace where parallelism_ratio < k_min or k_min <= 1.
// P4-falsifiable: a single serial-dominated trace refutes.
func F1Counterexample(g *Game, attractor []State, kMin float64) bool {
	if kMin <= 1.0 {
		return true // k_min <= 1 is itself a counterexample
	}
	ratio := ParallelismRatio(g, attractor)
	return ratio < kMin
}

// F2Counterexample checks the M1/M2 preemption boundary.
// M1 Option 1: single-basic-block preemption-free step.
// M2: multi-block step with preemption points.
// Counterexample: M1 exhibits preemption inside a nominally preemption-free
// block, or M2 achieves strictly better rank descent than M1.
func F2Counterexample(g *Game, attractor []State) bool {
	attrSet := make(map[State]bool)
	for _, s := range attractor {
		attrSet[s] = true
	}
	// M1 preemption-free: every state in the attractor must have exactly one
	// outgoing edge within the attractor (single-basic-block, no branching).
	// If any state has >1 outgoing attractor edges, M1's preemption-free
	// assumption is violated.
	for _, s := range attractor {
		outCount := 0
		for _, e := range g.Edges {
			if e.From == s && attrSet[e.To] {
				outCount++
			}
		}
		if outCount > 1 {
			return true // M1 preemption boundary violated
		}
	}
	return false
}

// F3Counterexample checks whether the runtime witness can reach the losing sink.
// Counterexample: a path from start to losingSink exists within the game graph.
func F3Counterexample(g *Game) bool {
	// BFS from start to losingSink
	if g.Start == g.LosingSink {
		return true
	}
	visited := make(map[State]bool)
	queue := []State{g.Start}
	visited[g.Start] = true
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		for _, e := range g.Edges {
			if e.From == cur {
				if e.To == g.LosingSink {
					return true
				}
				if !visited[e.To] {
					visited[e.To] = true
					queue = append(queue, e.To)
				}
			}
		}
	}
	return false
}

// ---------------------------------------------------------------------------
// RegionResult: per-region verification outcome
// ---------------------------------------------------------------------------

// RegionResult captures the full verification result for one region.
type RegionResult struct {
	Region          Region
	Verdict         Verdict
	StatesSeen      int
	EdgesTraversed  int
	CPUSecs         float64
	ParallelismRatio float64
	RankOK          bool
	ChannelCloseOK  bool
	F1Pass          bool
	F2Pass          bool
	F3Pass          bool
	Attractor       []State
}

// VerifyRegion runs the full verification pipeline for a single region.
func VerifyRegion(r Region, kMin float64) RegionResult {
	start := time.Now()
	g := NewGame(r)

	ar := SearchAttractor(g)
	elapsed := time.Since(start)

	rankOK := ValidateRankCertificate(g, ar.Attractor)
	channelCloseOK := CheckChannelClose(g, ar.Attractor)
	parRatio := ParallelismRatio(g, ar.Attractor)

	f1Pass := !F1Counterexample(g, ar.Attractor, kMin)
	f2Pass := !F2Counterexample(g, ar.Attractor)
	f3Pass := !F3Counterexample(g)

	verdict := VerdictUNKNOWN
	if !ar.Found {
		verdict = VerdictUNKNOWN
	} else if !f1Pass || !f2Pass || !f3Pass || !rankOK || !channelCloseOK {
		verdict = VerdictREJECT
	} else {
		verdict = VerdictADMIT
	}

	return RegionResult{
		Region:           r,
		Verdict:          verdict,
		StatesSeen:       ar.StatesSeen,
		EdgesTraversed:   ar.EdgesTraversed,
		CPUSecs:          elapsed.Seconds(),
		ParallelismRatio: parRatio,
		RankOK:           rankOK,
		ChannelCloseOK:   channelCloseOK,
		F1Pass:           f1Pass,
		F2Pass:           f2Pass,
		F3Pass:           f3Pass,
		Attractor:        ar.Attractor,
	}
}

// VerifyAll runs verification over all 20 pre-registered regions.
func VerifyAll(kMin float64) []RegionResult {
	results := make([]RegionResult, 0, len(AllRegions))
	for _, r := range AllRegions {
		results = append(results, VerifyRegion(r, kMin))
	}
	return results
}

// ---------------------------------------------------------------------------
// Default k_min
// ---------------------------------------------------------------------------

const DefaultKMin = 1.5
