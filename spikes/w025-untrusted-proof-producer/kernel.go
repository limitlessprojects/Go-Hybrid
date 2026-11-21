package w025

import (
	"fmt"
)

// CFGEdge represents a control flow graph edge in typed Go.
type CFGEdge struct {
	From string `json:"from"`
	To   string `json:"to"`
	Type string `json:"type"` // e.g., "branch", "spawn", "send", "step"
}

// MemoryOp represents an addressable memory operation.
type MemoryOp struct {
	Addr    string `json:"addr"`
	Op      string `json:"op"` // "alloc", "read", "write", "send"
	Task    string `json:"task"`
	IsWrite bool   `json:"is_write"`
}

// OwnershipInfo tracks owner identity and freshness.
type OwnershipInfo struct {
	Owner   string `json:"owner"`
	IsFresh bool   `json:"is_fresh"`
}

// SliceView represents an interval projection over an underlying allocation root.
type SliceView struct {
	ViewID string `json:"view_id"`
	RootID string `json:"root_id"`
	Start  int    `json:"start"`
	End    int    `json:"end"`
}

// Proof is the candidate permission derivation produced by hy-check.
type Proof struct {
	CFGEdges   []CFGEdge                `json:"cfg_edges"`
	MemoryOps  []MemoryOp               `json:"memory_ops"`
	Ownership  map[string]OwnershipInfo `json:"ownership"`
	Slices     []SliceView              `json:"slices"`
	Transfers  []TransferEvent          `json:"transfers"`
}

// TransferEvent represents an ownership handoff between tasks.
type TransferEvent struct {
	FromTask string `json:"from_task"`
	ToTask   string `json:"to_task"`
	ViewID   string `json:"view_id"`
}

// VerifierKernel independently checks untrusted proofs against the actual Go CFG and memory operations.
type VerifierKernel struct {
	Roots     map[string]int
	Views     map[string]SliceView
	TaskViews map[string]map[string]bool
}

// NewVerifierKernel creates a zero-SMT, direct deterministic verifier instance.
func NewVerifierKernel() *VerifierKernel {
	return &VerifierKernel{
		Roots:     make(map[string]int),
		Views:     make(map[string]SliceView),
		TaskViews: make(map[string]map[string]bool),
	}
}

// VerifyProof verifies that candidate proof covers every actual CFG edge and memory operation without races.
func (k *VerifierKernel) VerifyProof(p Proof, actualCFG []CFGEdge, actualOps []MemoryOp) error {
	// 1. Verify complete CFG coverage
	proofEdgeSet := make(map[string]bool)
	for _, e := range p.CFGEdges {
		proofEdgeSet[fmt.Sprintf("%s->%s:%s", e.From, e.To, e.Type)] = true
	}
	for _, e := range actualCFG {
		key := fmt.Sprintf("%s->%s:%s", e.From, e.To, e.Type)
		if !proofEdgeSet[key] {
			return fmt.Errorf("ERR_CFG_COVERAGE_MISMATCH: actual CFG edge %s not covered in proof", key)
		}
	}

	// 2. Register slices and validate bounds
	for _, s := range p.Slices {
		if s.Start < 0 || s.End < s.Start {
			return fmt.Errorf("ERR_FORGED_CERTIFICATE_INTEGRITY: invalid slice bounds [%d:%d]", s.Start, s.End)
		}
		k.Views[s.ViewID] = s
	}

	// 3. Verify memory operation alignment
	proofOps := make(map[string]bool)
	for _, op := range p.MemoryOps {
		proofOps[fmt.Sprintf("%s:%s:%s", op.Addr, op.Op, op.Task)] = true
	}
	for _, op := range actualOps {
		key := fmt.Sprintf("%s:%s:%s", op.Addr, op.Op, op.Task)
		if !proofOps[key] {
			return fmt.Errorf("ERR_MEMORY_OP_MISMATCH: actual op %s omitted from proof", key)
		}
	}

	// 4. Verify transfers and reject surviving aliases
	for _, tr := range p.Transfers {
		v, exists := k.Views[tr.ViewID]
		if !exists {
			return fmt.Errorf("ERR_FORGED_CERTIFICATE_INTEGRITY: transfer refers to unknown view %s", tr.ViewID)
		}

		// Check retained views in from_task
		for otherID := range k.TaskViews[tr.FromTask] {
			if otherID == tr.ViewID {
				continue
			}
			ov := k.Views[otherID]
			if ov.RootID == v.RootID {
				// Overlap check
				if max(v.Start, ov.Start) < min(v.End, ov.End) {
					return fmt.Errorf("ERR_SURVIVING_ALIAS_AFTER_TRANSFER: task %s retains overlapping alias %s [%d:%d]", tr.FromTask, otherID, ov.Start, ov.End)
				}
			}
		}
	}

	return nil
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
