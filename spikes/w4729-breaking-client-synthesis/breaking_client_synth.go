// Package breaking_client_synth implements directional ownership contract comparison
// and minimal downstream breaking client synthesis for Go-Hybrid APIs.
//
// Answering Proposal #4729: "[wildcard] Make an ownership-breaking release produce its smallest downstream client"
package breaking_client_synth

import (
	"fmt"
	"strings"
)

// OwnershipEffect classifies the compile-time ownership transition imposed on a parameter.
type OwnershipEffect int

const (
	// EffectPreserves borrows the argument; the caller retains full ownership and reuse rights.
	EffectPreserves OwnershipEffect = iota
	// EffectConsumes moves the argument into the callee; caller loses reuse rights (linear sink).
	EffectConsumes
	// EffectProduces creates a new affine value from nothing (constructor/factory).
	EffectProduces
	// EffectUnsupported marks operations involving raw pointer aliasing, callbacks, or escaping loans.
	EffectUnsupported
)

func (e OwnershipEffect) String() string {
	switch e {
	case EffectPreserves:
		return "PRESERVES (Borrow)"
	case EffectConsumes:
		return "CONSUMES (Move)"
	case EffectProduces:
		return "PRODUCES (Source)"
	case EffectUnsupported:
		return "UNSUPPORTED (Aliased/Callback)"
	default:
		return "UNKNOWN"
	}
}

// MethodSig represents a method or function declaration in an API contract.
type MethodSig struct {
	Name       string
	ParamType  string
	ReturnType string
	Effect     OwnershipEffect
}

// ContractRevision represents a set of exported methods at a specific git revision.
type ContractRevision struct {
	Revision string
	Methods  map[string]MethodSig
}

// NewContract creates an empty contract revision.
func NewContract(rev string) *ContractRevision {
	return &ContractRevision{
		Revision: rev,
		Methods:  make(map[string]MethodSig),
	}
}

// AddMethod registers an exported method in the contract revision.
func (c *ContractRevision) AddMethod(name, paramType, retType string, effect OwnershipEffect) {
	c.Methods[name] = MethodSig{
		Name:       name,
		ParamType:  paramType,
		ReturnType: retType,
		Effect:     effect,
	}
}

// ValueState models the compile-time affine linearity state of a variable.
type ValueState int

const (
	StateLive ValueState = iota
	StateConsumed
	StatePoisoned
)

func (s ValueState) String() string {
	switch s {
	case StateLive:
		return "LIVE"
	case StateConsumed:
		return "CONSUMED"
	case StatePoisoned:
		return "POISONED"
	default:
		return "INVALID"
	}
}

// StepOutcome models the result of invoking an operation on a variable in a specific state.
type StepOutcome struct {
	NewState ValueState
	Accepted bool
	DiagCode string // e.g. "HYB-E3003" for use-after-move
}

// ApplyStep executes a single method call against a current value state.
func ApplyStep(effect OwnershipEffect, cur ValueState) StepOutcome {
	switch effect {
	case EffectPreserves:
		if cur == StateConsumed {
			return StepOutcome{NewState: StatePoisoned, Accepted: false, DiagCode: "HYB-E3003"}
		}
		return StepOutcome{NewState: StateLive, Accepted: true, DiagCode: ""}
	case EffectConsumes:
		if cur == StateConsumed {
			return StepOutcome{NewState: StatePoisoned, Accepted: false, DiagCode: "HYB-E3003"}
		}
		return StepOutcome{NewState: StateConsumed, Accepted: true, DiagCode: ""}
	case EffectProduces:
		return StepOutcome{NewState: StateLive, Accepted: true, DiagCode: ""}
	case EffectUnsupported:
		return StepOutcome{NewState: StatePoisoned, Accepted: false, DiagCode: "UNKNOWN"}
	default:
		return StepOutcome{NewState: StatePoisoned, Accepted: false, DiagCode: "INTERNAL_ERROR"}
	}
}

// SynthesisVerdict characterizes the compatibility relationship between two releases.
type SynthesisVerdict string

const (
	VerdictBreaking    SynthesisVerdict = "BREAKING_CHANGE_WITNESS_GENERATED"
	VerdictRelaxation  SynthesisVerdict = "CONTRACT_RELAXATION (ADMISSIBLE)"
	VerdictEquivalent  SynthesisVerdict = "IDENTICAL_CONTRACT (NO_WITNESS)"
	VerdictUnsupported SynthesisVerdict = "UNKNOWN (UNSUPPORTED_ALIASING)"
)

// BreakingWitness represents a synthesized minimal client that passes under Old and fails under New.
type BreakingWitness struct {
	Verdict        SynthesisVerdict
	ShortestPath   []string
	OldPasses      bool
	NewRejectsAt   int
	NewDiagCode    string
	SynthesizedHy  string
	TotalOldPasses int
	TotalNewFails  int
}

// SynthesizeMinimalWitness executes a breadth-first search over straight-line call sequences
// up to maxDepth to find the shortest witness where Old(seq) == ACCEPT and New(seq) == REJECT.
func SynthesizeMinimalWitness(oldContract, newContract *ContractRevision, maxDepth int) BreakingWitness {
	// 1. Check for unsupported aliasing
	for _, m := range newContract.Methods {
		if m.Effect == EffectUnsupported {
			return BreakingWitness{
				Verdict:     VerdictUnsupported,
				NewDiagCode: "UNKNOWN",
			}
		}
	}
	for _, m := range oldContract.Methods {
		if m.Effect == EffectUnsupported {
			return BreakingWitness{
				Verdict:     VerdictUnsupported,
				NewDiagCode: "UNKNOWN",
			}
		}
	}

	// 2. Collect common method names
	var methodNames []string
	for name := range oldContract.Methods {
		if _, exists := newContract.Methods[name]; exists {
			methodNames = append(methodNames, name)
		}
	}

	// 3. BFS search over sequences
	type QueueItem struct {
		Path     []string
		OldState ValueState
		NewState ValueState
	}

	queue := []QueueItem{{Path: []string{}, OldState: StateLive, NewState: StateLive}}
	var foundWitness []string
	rejectIndex := -1
	diagCode := ""

	totalOldAccepted := 0
	totalNewRejected := 0

	// Enumeration up to maxDepth
	for len(queue) > 0 {
		curr := queue[0]
		queue = queue[1:]

		if len(curr.Path) >= maxDepth {
			continue
		}

		for _, mName := range methodNames {
			nextPath := append(append([]string{}, curr.Path...), mName)

			oldEff := oldContract.Methods[mName].Effect
			newEff := newContract.Methods[mName].Effect

			oldStep := ApplyStep(oldEff, curr.OldState)
			newStep := ApplyStep(newEff, curr.NewState)

			if oldStep.Accepted {
				totalOldAccepted++
				if !newStep.Accepted {
					totalNewRejected++
					if foundWitness == nil {
						foundWitness = nextPath
						rejectIndex = len(nextPath) - 1
						diagCode = newStep.DiagCode
					}
				} else {
					// Both accepted, continue BFS
					queue = append(queue, QueueItem{
						Path:     nextPath,
						OldState: oldStep.NewState,
						NewState: newStep.NewState,
					})
				}
			}
		}
	}

	if foundWitness != nil {
		hyCode := GenerateHybridClient(foundWitness, rejectIndex, diagCode, oldContract.Methods[foundWitness[0]].ParamType)
		return BreakingWitness{
			Verdict:        VerdictBreaking,
			ShortestPath:   foundWitness,
			OldPasses:      true,
			NewRejectsAt:   rejectIndex,
			NewDiagCode:    diagCode,
			SynthesizedHy:  hyCode,
			TotalOldPasses: totalOldAccepted,
			TotalNewFails:  totalNewRejected,
		}
	}

	// If no breaking witness found, check if new contract is a relaxation or identical
	isRelaxation := false
	isDifferent := false
	for _, name := range methodNames {
		o := oldContract.Methods[name].Effect
		n := newContract.Methods[name].Effect
		if o != n {
			isDifferent = true
			if o == EffectConsumes && n == EffectPreserves {
				isRelaxation = true
			}
		}
	}

	if isRelaxation {
		return BreakingWitness{
			Verdict: VerdictRelaxation,
		}
	}
	if !isDifferent {
		return BreakingWitness{
			Verdict: VerdictEquivalent,
		}
	}

	return BreakingWitness{
		Verdict: VerdictEquivalent,
	}
}

// GenerateHybridClient synthesizes a concrete .hy file reproducing the breaking contract violation.
func GenerateHybridClient(path []string, rejectAt int, diag string, paramType string) string {
	if paramType == "" {
		paramType = "ResourceHandle"
	}

	var sb strings.Builder
	sb.WriteString("// Auto-generated by breaking_client_synth (Issue #4729)\n")
	sb.WriteString("// Reproduces downstream compiler rejection on candidate release.\n")
	sb.WriteString("package downstream_witness\n\n")
	sb.WriteString("import \"api\"\n\n")
	sb.WriteString(fmt.Sprintf("pub fn witness_breaking(x %s) {\n", paramType))

	for i, step := range path {
		if i == rejectAt {
			sb.WriteString(fmt.Sprintf("    // Rejection expected here under v2 (Compiler Diagnostic: %s)\n", diag))
			sb.WriteString(fmt.Sprintf("    api.%s(x)\n", step))
		} else {
			sb.WriteString(fmt.Sprintf("    api.%s(x)\n", step))
		}
	}
	sb.WriteString("}\n")
	return sb.String()
}
