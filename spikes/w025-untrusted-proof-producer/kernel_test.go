package w025

import (
	"testing"
)

func TestSupportedUniqueTransfer(t *testing.T) {
	kernel := NewVerifierKernel()
	proof := Proof{
		CFGEdges: []CFGEdge{
			{From: "main:0", To: "main:spawn", Type: "spawn"},
			{From: "main:spawn", To: "worker:run", Type: "step"},
		},
		MemoryOps: []MemoryOp{
			{Addr: "r1", Op: "alloc", Task: "main", IsWrite: true},
			{Addr: "v_x", Op: "write", Task: "worker", IsWrite: true},
		},
		Slices: []SliceView{
			{ViewID: "v_x", RootID: "r1", Start: 0, End: 4},
		},
		Transfers: []TransferEvent{
			{FromTask: "main", ToTask: "worker", ViewID: "v_x"},
		},
	}

	actualCFG := []CFGEdge{
		{From: "main:0", To: "main:spawn", Type: "spawn"},
		{From: "main:spawn", To: "worker:run", Type: "step"},
	}

	actualOps := []MemoryOp{
		{Addr: "r1", Op: "alloc", Task: "main", IsWrite: true},
		{Addr: "v_x", Op: "write", Task: "worker", IsWrite: true},
	}

	if err := kernel.VerifyProof(proof, actualCFG, actualOps); err != nil {
		t.Fatalf("Expected valid transfer to be accepted, got error: %v", err)
	}
}

func TestSurvivingAliasWritesAfterTransfer(t *testing.T) {
	kernel := NewVerifierKernel()
	kernel.TaskViews["main"] = map[string]bool{"v_x": true, "v_alias": true}
	proof := Proof{
		CFGEdges: []CFGEdge{
			{From: "main:0", To: "main:1", Type: "step"},
		},
		Slices: []SliceView{
			{ViewID: "v_x", RootID: "r1", Start: 0, End: 4},
			{ViewID: "v_alias", RootID: "r1", Start: 0, End: 4},
		},
		Transfers: []TransferEvent{
			{FromTask: "main", ToTask: "worker", ViewID: "v_x"},
		},
	}

	actualCFG := []CFGEdge{
		{From: "main:0", To: "main:1", Type: "step"},
	}

	err := kernel.VerifyProof(proof, actualCFG, nil)
	if err == nil {
		t.Fatalf("Expected surviving alias error, got nil")
	}
}
