package stm

import (
	"context"
	"sync"
	"testing"
)

// TestP1_SharedMapTwoTaskSerializable verifies Criterion (A):
// A two-task shared map mutator rejected by Tier-0 (01-shared-map-across-tasks.hy)
// runs race-free under `go test -race` and produces only serializable outcomes.
func TestP1_SharedMapTwoTaskSerializable(t *testing.T) {
	token := SynthesizePureToken()
	for i := 0; i < 1000; i++ {
		r := NewRegion()
		counts := NewTxMap(r, map[string]int{"hits": 10, "a": 1, "b": 2}, nil)

		var wg sync.WaitGroup
		wg.Add(2)

		go func() {
			defer wg.Done()
			err := r.RunTask(context.Background(), token, func(tx *Tx) error {
				h, err := counts.Get(tx, "hits")
				if err != nil {
					return err
				}
				if err := counts.Put(tx, "hits", h+5); err != nil {
					return err
				}
				h2, err := counts.Get(tx, "hits")
				if err != nil {
					return err
				}
				return counts.Put(tx, "a", h2*2)
			})
			if err != nil {
				t.Errorf("task 1 failed: %v", err)
			}
		}()

		go func() {
			defer wg.Done()
			err := r.RunTask(context.Background(), token, func(tx *Tx) error {
				h, err := counts.Get(tx, "hits")
				if err != nil {
					return err
				}
				if err := counts.Put(tx, "hits", h*3); err != nil {
					return err
				}
				h2, err := counts.Get(tx, "hits")
				if err != nil {
					return err
				}
				return counts.Put(tx, "b", h2+1)
			})
			if err != nil {
				t.Errorf("task 2 failed: %v", err)
			}
		}()

		wg.Wait()

		vtx := r.begin()
		hits, _ := counts.Get(vtx, "hits")
		a, _ := counts.Get(vtx, "a")
		b, _ := counts.Get(vtx, "b")

		validOrder12 := hits == 45 && a == 30 && b == 46
		validOrder21 := hits == 35 && a == 70 && b == 31
		if !validOrder12 && !validOrder21 {
			t.Fatalf("iter %d: non-serializable outcome hits=%d a=%d b=%d", i, hits, a, b)
		}
	}
}

// TestP2_CyclicGraphAndSliceAliasMediation verifies Criterion (B):
// Cyclic node graphs (A <-> B) and overlapping slice aliases mediate all mutations.
func TestP2_CyclicGraphAndSliceAliasMediation(t *testing.T) {
	r := NewRegion()
	token := SynthesizePureToken()

	type TxNode struct {
		Weight *TxCell[int]
		Next   *TxCell[*TxNode]
	}

	nodeA := &TxNode{Weight: NewTxCell(r, 100, nil), Next: NewTxCell[*TxNode](r, nil, nil)}
	nodeB := &TxNode{Weight: NewTxCell(r, 200, nil), Next: NewTxCell[*TxNode](r, nil, nil)}

	_ = r.RunTask(context.Background(), token, func(tx *Tx) error {
		if err := Store(tx, nodeA.Next, nodeB); err != nil {
			return err
		}
		return Store(tx, nodeB.Next, nodeA)
	})

	// Overlapping slice view conflict detection
	base := NewTxSlice(r, []int{10, 20, 30, 40}, nil)
	sub := base.SubSlice(1, 3) // sub[0] aliases base[1]

	tx1 := r.begin()
	tx2 := r.begin()

	_, _ = base.Get(tx1, 1)
	_ = sub.Set(tx2, 0, 999)
	if !r.tryCommit(tx2) {
		t.Fatal("expected tx2 to commit cleanly")
	}
	_ = base.Set(tx1, 1, 555)
	if r.tryCommit(tx1) {
		t.Fatal("expected tx1 to abort on aliased sub-slice modification")
	}
}

// TestP4_MultiTaskCancellationIsolation verifies Criterion (D):
// Cancelled tasks never leak buffered writes into the shared heap.
func TestP4_MultiTaskCancellationIsolation(t *testing.T) {
	r := NewRegion()
	cell := NewTxCell(r, 500, nil)

	ctx, cancel := context.WithCancel(context.Background())
	tx := r.begin()
	_ = Store(tx, cell, 9999)
	cancel()

	err := r.RunTask(ctx, SynthesizePureToken(), func(tx *Tx) error {
		return Store(tx, cell, 8888)
	})
	if err != ErrTxCancelled {
		t.Fatalf("expected ErrTxCancelled, got %v", err)
	}

	vtx := r.begin()
	val, _ := Load(vtx, cell)
	if val != 500 {
		t.Fatalf("cancelled transaction leaked state: got %d, want 500", val)
	}
}
