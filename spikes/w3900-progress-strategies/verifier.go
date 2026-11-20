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
//
// Generation is fully deterministic (seeded, zero external randomness) so
// pre-registration is meaningful and every region is reproducible.
package w3900progressstrategies

import (
	"fmt"
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
// Out is an optional adjacency index: when non-nil, outEdges(s) returns
// g.Out[s] in O(1); when nil, outEdges falls back to a linear scan of
// g.Edges so bare &Game{...} literals in unit tests keep working.
type Game struct {
	States        []State
	Edges         []Edge
	Start         State
	Terminal      []State // attractor terminal states
	LosingSink    State   // explicit losing sink (F3 witness target)
	ChannelCloses []ChannelClose
	Rank          map[State]int // rank certificate per state
	Out           map[State][]Edge
}

// outEdges returns the outgoing edges of state s, using the adjacency index
// when present and falling back to a linear scan of g.Edges otherwise.
func (g *Game) outEdges(s State) []Edge {
	if g.Out != nil {
		return g.Out[s]
	}
	var out []Edge
	for _, e := range g.Edges {
		if e.From == s {
			out = append(out, e)
		}
	}
	return out
}

// compileOut rebuilds the adjacency index from g.Edges.
func compileOut(g *Game) {
	g.Out = make(map[State][]Edge, len(g.States))
	for _, e := range g.Edges {
		g.Out[e.From] = append(g.Out[e.From], e)
	}
}

// ---------------------------------------------------------------------------
// Deterministic game generator (seeded, zero external randomness)
// ---------------------------------------------------------------------------

// NewGame builds a deterministic finite-state game for a region.
// All Verdicts are produced by VerifyRegion through SearchAttractor,
// ValidateRankCertificate, CheckChannelClose and the F1/F2/F3 falsifiers;
// no `if r.Valid { return VerdictADMIT }` cheat path exists.
func NewGame(r Region) *Game {
	if r.Valid {
		return buildValid(r)
	}
	return buildInvalid(r)
}

// buildValid constructs a valid region (R01–R10): a bounded deterministic
// progress chain 0 -> 1 -> ... -> chainLen-1 where every non-terminal state
// emits k_i parallel edges (all to the same successor State(i+1), i.e. data
// parallelism inside a single basic block, not control-flow divergence).
//
//   - terminal = State(chainLen - 1), Rank[terminal] = 0
//   - Rank[State(i)] = (chainLen - 1 - i) * 8, so rf - rt = 8 >= e.Cost
//     holds for every edge (max e.Cost is 5).
//   - losingSink = State(chainLen), an independent sink with no in-edges,
//     so F3Counterexample BFS returns false.
//   - R01 (seed 0x01) has no ChannelCloses; R02–R10 close only at terminal.
//   - R06 (seed 0x06) forces chainLen = 6 with the first 3 states having 2
//     parallel edges and the last 2 states having 1 edge, yielding
//     ParallelismRatio == 3.0 / 2.0 == 1.5 exactly (the k_min boundary).
func buildValid(r Region) *Game {
	seed := int(r.Seed)
	chainLen := 6 + seed%5
	if seed == 0x06 {
		chainLen = 6
	}
	terminal := State(chainLen - 1)
	losingSink := State(chainLen)

	states := make([]State, chainLen+1) // 0..chainLen inclusive (losingSink)
	for i := range states {
		states[i] = State(i)
	}

	var edges []Edge
	rank := make(map[State]int, chainLen)
	for i := State(0); i < terminal; i++ {
		rank[i] = (int(terminal) - int(i)) * 8
		k := 2
		if seed == 0x06 && i >= 3 {
			k = 1
		}
		for lane := 0; lane < k; lane++ {
			cost := 1 + (seed+int(i)+lane)%5
			edges = append(edges, Edge{From: i, To: i + 1, Cost: cost})
		}
	}
	rank[terminal] = 0

	var channelCloses []ChannelClose
	if seed != 0x01 {
		channelCloses = []ChannelClose{{State: terminal, Closed: true}}
	}

	g := &Game{
		States:        states,
		Edges:         edges,
		Start:         0,
		Terminal:      []State{terminal},
		LosingSink:    losingSink,
		ChannelCloses: channelCloses,
		Rank:          rank,
	}
	compileOut(g)
	return g
}

// baseChain builds a plain serial chain 0 -> 1 -> ... -> chainLen-1 with a
// strictly decreasing rank certificate and an explicit losing sink at
// State(chainLen). Used as the skeleton for near-miss regions.
func baseChain(chainLen int) *Game {
	terminal := State(chainLen - 1)
	losingSink := State(chainLen)

	states := make([]State, chainLen+1)
	for i := range states {
		states[i] = State(i)
	}

	var edges []Edge
	rank := make(map[State]int, chainLen)
	for i := State(0); i < terminal; i++ {
		edges = append(edges, Edge{From: i, To: i + 1, Cost: 1})
		rank[i] = (int(terminal) - int(i)) * 8
	}
	rank[terminal] = 0

	g := &Game{
		States:   states,
		Edges:    edges,
		Start:    0,
		Terminal: []State{terminal},
		LosingSink: losingSink,
		Rank:     rank,
	}
	compileOut(g)
	return g
}

// addEdge appends a directed edge to both g.Edges and the adjacency index.
func addEdge(g *Game, from, to State, cost int) {
	g.Edges = append(g.Edges, Edge{From: from, To: to, Cost: cost})
	g.Out[from] = append(g.Out[from], Edge{From: from, To: to, Cost: cost})
}

// buildInvalid constructs a near-miss region (N01–N10) by injecting the
// pre-registered fault for mode = int(r.Seed) - 0x80 (1..10).
func buildInvalid(r Region) *Game {
	mode := int(r.Seed) - 0x80

	switch mode {
	case 1: // N01: rank plateau Rank[1] == Rank[2] -> rf - rt < e.Cost
		g := baseChain(6)
		g.Rank[1] = g.Rank[2]
		return g
	case 2: // N02: all single-lane serial chain -> ParallelismRatio = 0.0
		return baseChain(6)
	case 3: // N03: channel closed at non-terminal State(1) -> send-after-close
		g := baseChain(6)
		g.ChannelCloses = []ChannelClose{{State: 1, Closed: true}}
		return g
	case 4: // N04: edge Cost = 64 > Rank[i] - Rank[i+1]
		g := baseChain(6)
		g.Edges[2].Cost = 64
		compileOut(g)
		return g
	case 5: // N05: channel closed at start State(0) before drain
		g := baseChain(6)
		g.ChannelCloses = []ChannelClose{{State: 0, Closed: true}}
		return g
	case 6: // N06: missing rank certificate (empty Rank map)
		g := baseChain(6)
		g.Rank = map[State]int{}
		return g
	case 7: // N07: state count exceeds budget (chainLen = MaxStates + 10)
		return baseChain(MaxStates + 10)
	case 8: // N08: escape edge State(1) -> losingSink
		g := baseChain(6)
		addEdge(g, 1, g.LosingSink, 1)
		return g
	case 9: // N09: State(1) forks to two distinct successors State(2) and State(3)
		g := baseChain(6)
		addEdge(g, 1, 3, 1)
		return g
	case 10: // N10: escape edge State(chainLen-2) -> losingSink (chainLen == 6)
		g := baseChain(6)
		addEdge(g, 4, g.LosingSink, 1)
		return g
	default:
		return baseChain(6)
	}
}

// ---------------------------------------------------------------------------
// Attractor search (finite-state game)
// ---------------------------------------------------------------------------

// AttractorResult captures the outcome of attractor search.
type AttractorResult struct {
	Found         bool
	Attractor     []State
	StatesSeen    int
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
		// Fail-closed: never allow the counters to exceed the hard caps.
		if statesSeen >= MaxStates || edgesTraversed >= MaxEdges {
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

		// Expand edges via the adjacency index (O(1) when present).
		for _, e := range g.outEdges(s) {
			edgesTraversed++
			if !seen[e.To] {
				stack = append(stack, e.To)
			}
		}
	}

	// Build attractor: states that can reach a terminal.
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
		// Start state itself may be the only path to terminal.
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
		for _, e := range g.outEdges(cur) {
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
// Terminal states are treated as valid destinations: an edge from a
// non-terminal attractor state to a terminal state is a serial step.
func ParallelismRatio(g *Game, attractor []State) float64 {
	attrSet := make(map[State]bool)
	for _, s := range attractor {
		attrSet[s] = true
	}
	for _, t := range g.Terminal {
		attrSet[t] = true
	}

	concurrent := 0
	serial := 0
	for _, s := range attractor {
		outCount := 0
		for _, e := range g.outEdges(s) {
			if attrSet[e.To] {
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
			for _, e := range g.outEdges(cc.State) {
				if attrSet[e.To] {
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
// Counterexample: a non-terminal state in the attractor branches to >= 2
// distinct successor states (control-flow divergence / preemption). Multiple
// edges pointing to the same successor (distinctTargets == 1) are data
// parallelism inside one basic block and do not trigger F2.
func F2Counterexample(g *Game, attractor []State) bool {
	attrSet := make(map[State]bool)
	for _, s := range attractor {
		attrSet[s] = true
	}
	for _, s := range attractor {
		targets := make(map[State]bool)
		for _, e := range g.outEdges(s) {
			if attrSet[e.To] {
				targets[e.To] = true
			}
		}
		if len(targets) > 1 {
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
		for _, e := range g.outEdges(cur) {
			if e.To == g.LosingSink {
				return true
			}
			if !visited[e.To] {
				visited[e.To] = true
				queue = append(queue, e.To)
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

	// V1-c: missing certificate or exhausted budget -> UNKNOWN, no claim.
	verdict := VerdictUNKNOWN
	if !ar.Found {
		verdict = VerdictUNKNOWN
	} else if len(g.Rank) == 0 {
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
