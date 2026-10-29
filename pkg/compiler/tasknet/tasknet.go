package tasknet

import (
	"fmt"
)

// Edge represents a directed connection between actors with fixed rates.
type Edge struct {
	ID            string
	Source        string
	Target        string
	ProduceRate   int
	ConsumeRate   int
	InitialTokens int
}

// Actor represents a functional unit in the task network.
type Actor struct {
	ID string
}

// Schedule contains the execution witness and buffer constraints.
type Schedule struct {
	FiringOrder []string       // Sequence of actor IDs to execute in one cycle
	Budgets     map[string]int // Edge ID -> Maximum allowed tokens (high-water mark)
}

// Network defines the topology of the fixed-rate system.
type Network struct {
	Actors map[string]*Actor
	Edges  map[string]*Edge
}

// Verify validates that the schedule is safe and conservative.
// It ensures no buffer underflows, no overflows beyond budgets, and that
// the final state matches the initial state (balance equations).
func Verify(net *Network, sched *Schedule) error {
	tokens := make(map[string]int)
	for id, edge := range net.Edges {
		tokens[id] = edge.InitialTokens
	}

	for step, actorID := range sched.FiringOrder {
		// Check consumption
		for id, edge := range net.Edges {
			if edge.Target == actorID {
				if tokens[id] < edge.ConsumeRate {
					return fmt.Errorf("step %d: actor %s underflow on edge %s (have %d, need %d)", step, actorID, id, tokens[id], edge.ConsumeRate)
				}
				tokens[id] -= edge.ConsumeRate
			}
		}

		// Check production
		for id, edge := range net.Edges {
			if edge.Source == actorID {
				tokens[id] += edge.ProduceRate
				if budget, ok := sched.Budgets[id]; ok && tokens[id] > budget {
					return fmt.Errorf("step %d: actor %s overflow on edge %s (have %d, budget %d)", step, actorID, id, tokens[id], budget)
				}
			}
		}
	}

	// Check conservation
	for id, edge := range net.Edges {
		if tokens[id] != edge.InitialTokens {
			return fmt.Errorf("unbalanced schedule: edge %s ended with %d tokens, expected %d", id, tokens[id], edge.InitialTokens)
		}
	}

	return nil
}
