package periodic

import (
	"fmt"
	"reflect"
	"testing"
)

// prefixOracle uses a closed-form prefix equation, NOT Verify's mutable
// consume/produce replay. Occupancy after a prefix is I + p*Nfrom - c*Nto.
// Input availability is tested BEFORE incrementing the firing count, including
// for self-loops; testing only the net change would incorrectly admit them.
type oracleResult struct {
	safe, periodic bool
	counts, high   []int
	marking        []int
}

func prefixOracle(g Graph, caps, schedule []int) oracleResult {
	r := oracleResult{safe: true, periodic: true, counts: make([]int, len(g.Actors)), high: make([]int, len(g.Edges)), marking: make([]int, len(g.Edges))}
	for i, e := range g.Edges {
		r.high[i], r.marking[i] = len(e.Initial), len(e.Initial)
		if caps[i] < 1 || len(e.Initial) > caps[i] {
			r.safe = false
		}
	}
	for _, a := range schedule {
		for _, e := range g.Edges {
			before := len(e.Initial) + e.Produce*r.counts[e.From] - e.Consume*r.counts[e.To]
			if e.To == a && before < e.Consume {
				r.safe = false
			}
			// The other edge's consumption cannot make THIS edge's production
			// fit. Each FIFO is independently bounded.
		}
		r.counts[a]++
		for i, e := range g.Edges {
			after := len(e.Initial) + e.Produce*r.counts[e.From] - e.Consume*r.counts[e.To]
			r.marking[i] = after
			if after < 0 || after > caps[i] {
				r.safe = false
			}
			if after > r.high[i] {
				r.high[i] = after
			}
		}
	}
	for _, n := range r.counts {
		if n == 0 {
			r.periodic = false
		}
	}
	for i, e := range g.Edges {
		if r.marking[i] != len(e.Initial) {
			r.periodic = false
		}
	}
	return r
}

func oracleCertificate(g Graph, caps, schedule []int, r oracleResult) Certificate {
	var payload uint64
	for _, n := range caps {
		payload += uint64(n) * 8
	}
	return Certificate{Digest(g), schedule, r.counts, caps, r.high, payload}
}

func smallGraphs() []Graph {
	a := func(name, op string) Actor { return Actor{Name: name, Op: op, Factor: 0} }
	e := func(name string, from, to, p, c, n int) Edge {
		return Edge{Name: name, From: from, To: to, Produce: p, Consume: c, Initial: make([]uint64, n)}
	}
	return []Graph{
		{[]Actor{a("s", "source"), a("t", "sink")}, []Edge{e("x", 0, 1, 1, 1, 0)}},
		{[]Actor{a("s", "source"), a("t", "sink")}, []Edge{e("x", 0, 1, 2, 1, 0)}},
		{[]Actor{a("s", "source"), a("t", "sink")}, []Edge{e("x", 0, 1, 1, 2, 1)}},
		{[]Actor{a("a", "sum"), a("b", "sum")}, []Edge{e("x", 0, 1, 1, 1, 1), e("y", 1, 0, 1, 1, 0)}},
		{[]Actor{a("a", "sum"), a("b", "sum")}, []Edge{e("x", 0, 1, 2, 1, 1), e("y", 1, 0, 1, 2, 1)}},
		{[]Actor{a("a", "sum"), a("b", "sum")}, []Edge{e("x", 0, 1, 1, 1, 0), e("y", 1, 0, 1, 1, 0)}},
		{[]Actor{a("a", "sum")}, []Edge{e("x", 0, 0, 1, 1, 0)}},
		{[]Actor{a("a", "sum")}, []Edge{e("x", 0, 0, 1, 1, 1)}},
		{[]Actor{a("a", "sum")}, []Edge{e("x", 0, 0, 2, 2, 1)}},
		{[]Actor{a("a", "sum")}, []Edge{e("x", 0, 0, 2, 2, 2)}},
		{[]Actor{a("a", "sum")}, []Edge{e("x", 0, 0, 2, 1, 1)}},
		{[]Actor{a("a", "source"), a("b", "sum"), a("c", "sink")}, []Edge{e("x", 0, 1, 1, 1, 0), e("y", 1, 2, 1, 1, 0)}},
	}
}

func eachCapacity(edges int, f func([]int)) {
	caps := make([]int, edges)
	var visit func(int)
	visit = func(i int) {
		if i == edges {
			f(append([]int(nil), caps...))
			return
		}
		for n := 0; n <= 3; n++ {
			caps[i] = n
			visit(i + 1)
		}
	}
	visit(0)
}

func TestExhaustiveWitnesses(t *testing.T) {
	checked, accepted := 0, 0
	for gi, g := range smallGraphs() {
		eachCapacity(len(g.Edges), func(caps []int) {
			var visit func([]int)
			visit = func(schedule []int) {
				if len(schedule) > 0 {
					r := prefixOracle(g, caps, schedule)
					c := oracleCertificate(g, caps, schedule, r)
					report, err := Verify(g, c)
					want := r.safe && r.periodic
					checked++
					if (err == nil) != want {
						t.Fatalf("graph %d caps %v schedule %v: oracle %v, replay %v", gi, caps, schedule, want, err)
					}
					if err == nil {
						accepted++
						if !reflect.DeepEqual(report.HighWater, r.high) {
							t.Fatal("accepted high-water differs from independent oracle")
						}
					}
				}
				if len(schedule) < 6 {
					for a := range g.Actors {
						visit(append(schedule, a))
					}
				}
			}
			visit(nil)
		})
	}
	if accepted == 0 || accepted == checked {
		t.Fatal("enumeration lacks positive or negative controls")
	}
	t.Logf("exhaustive witnesses: %d checked, %d accepted, %d rejected (12 graph families, capacities 0..3, lengths 1..6)", checked, accepted, checked-accepted)
}

// The finite-state search is exhaustive over marking x fired-actor-mask,
// independent of the six-firing bound above. Counts in a representative path
// feed the prefix equation; equal marking/mask pairs have equal successors.
func TestReachableStateSpace(t *testing.T) {
	states, transitions, feasible, infeasible := 0, 0, 0, 0
	for _, g := range smallGraphs() {
		eachCapacity(len(g.Edges), func(caps []int) {
			initial := prefixOracle(g, caps, nil)
			if !initial.safe {
				return
			}
			key := func(r oracleResult) string {
				mask := 0
				for i, n := range r.counts {
					if n > 0 {
						mask |= 1 << i
					}
				}
				return fmt.Sprint(r.marking, "/", mask)
			}
			queue := [][]int{nil}
			seen := map[string]bool{key(initial): true}
			found := false
			for head := 0; head < len(queue); head++ {
				states++
				for a := range g.Actors {
					path := append(append([]int(nil), queue[head]...), a)
					r := prefixOracle(g, caps, path)
					transitions++
					if !r.safe {
						continue
					}
					if r.periodic {
						found = true
						if _, err := Verify(g, oracleCertificate(g, caps, path, r)); err != nil {
							t.Fatalf("reachable periodic witness rejected: %v", err)
						}
					}
					k := key(r)
					if !seen[k] {
						seen[k] = true
						queue = append(queue, path)
					}
				}
			}
			if found {
				feasible++
			} else {
				infeasible++
			}
		})
	}
	if feasible == 0 || infeasible == 0 {
		t.Fatal("state exploration lacks feasible or deadlocked cases")
	}
	t.Logf("reachable states: %d; attempted transitions: %d; feasible capacity assignments: %d; infeasible: %d", states, transitions, feasible, infeasible)
}
