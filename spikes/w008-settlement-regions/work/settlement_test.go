package w008

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

type MockResource struct {
	id       string
	released atomic.Bool
}

func newResource(id string) *MockResource {
	return &MockResource{id: id}
}

func (m *MockResource) release() {
	m.released.Store(true)
}

// Positive Trace 1: Success path — both children acquire, process, and transfer results into bundle.
func TestPositiveTrace_NormalCompletion(t *testing.T) {
	ctx := context.Background()
	region := NewRegion(ctx)

	r1 := newResource("res-1")
	r2 := newResource("res-2")

	slot1, err := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })
	if err != nil {
		t.Fatalf("RegisterHandle h1 failed: %v", err)
	}

	slot2, err := RegisterHandle(region, "h2", r2, func(r *MockResource) { r.release() })
	if err != nil {
		t.Fatalf("RegisterHandle h2 failed: %v", err)
	}

	// Spawn Child A
	region.SpawnTask(func(c context.Context) {
		h, ok := slot1.TryAcquire()
		if !ok {
			t.Errorf("Child A failed to acquire slot1")
			return
		}
		// Transfer ownership to result bundle
		if _, err := TransferResult(region, h); err != nil {
			t.Errorf("Child A TransferResult failed: %v", err)
		}
	})

	// Spawn Child B
	region.SpawnTask(func(c context.Context) {
		h, ok := slot2.TryAcquire()
		if !ok {
			t.Errorf("Child B failed to acquire slot2")
			return
		}
		// Transfer ownership to result bundle
		if _, err := TransferResult(region, h); err != nil {
			t.Errorf("Child B TransferResult failed: %v", err)
		}
	})

	report, err := region.WaitSettled()
	if err != nil {
		t.Fatalf("WaitSettled failed: %v", err)
	}

	if !report.Settled || report.TransferredCount != 2 || report.ReleasedCount != 0 {
		t.Fatalf("Unexpected report: %+v", report)
	}
}

// Positive Trace 2: Cancellation before child acquisition — parent cancels immediately.
// Atomic handoff CAS ensures cancellation wins; parent ledger releases escrowed handles.
func TestPositiveTrace_CancellationBeforeAcquisition(t *testing.T) {
	ctx := context.Background()
	region := NewRegion(ctx)

	r1 := newResource("res-1")
	r2 := newResource("res-2")

	slot1, err := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })
	if err != nil {
		t.Fatalf("RegisterHandle h1 failed: %v", err)
	}
	slot2, err := RegisterHandle(region, "h2", r2, func(r *MockResource) { r.release() })
	if err != nil {
		t.Fatalf("RegisterHandle h2 failed: %v", err)
	}

	// Parent cancels BEFORE children attempt acquisition
	region.Cancel()

	// Child tasks attempt acquisition and observe rejection
	childAcquiredCount := atomic.Int32{}
	region.SpawnTask(func(c context.Context) {
		if _, ok := slot1.TryAcquire(); ok {
			childAcquiredCount.Add(1)
		}
	})
	region.SpawnTask(func(c context.Context) {
		if _, ok := slot2.TryAcquire(); ok {
			childAcquiredCount.Add(1)
		}
	})

	report, err := region.WaitSettled()
	if err != nil {
		t.Fatalf("WaitSettled failed: %v", err)
	}

	if childAcquiredCount.Load() != 0 {
		t.Fatalf("Children should not have acquired cancelled slots")
	}
	if !report.Settled || report.ReleasedCount != 2 || report.TransferredCount != 0 {
		t.Fatalf("Expected all handles settled via release, got: %+v", report)
	}
	if !r1.released.Load() || !r2.released.Load() {
		t.Fatalf("Resources were not synchronously released")
	}
}

// Positive Trace 3: Cooperative cancellation after acquisition — children acquire,
// observe region cancellation, and synchronously release their owned handles.
func TestPositiveTrace_CooperativeCancellationAfterAcquisition(t *testing.T) {
	ctx := context.Background()
	region := NewRegion(ctx)

	r1 := newResource("res-1")
	slot1, _ := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })

	acquiredSig := make(chan struct{})
	continueSig := make(chan struct{})

	region.SpawnTask(func(c context.Context) {
		h, ok := slot1.TryAcquire()
		if !ok {
			t.Errorf("expected acquisition to succeed")
			return
		}
		close(acquiredSig)
		<-continueSig

		// Context is cancelled; child gracefully releases handle
		if c.Err() != nil {
			if _, err := ReleaseHandle(region, h); err != nil {
				t.Errorf("ReleaseHandle failed: %v", err)
			}
		}
	})

	<-acquiredSig
	region.Cancel() // Cancel after child has acquired
	close(continueSig)

	report, err := region.WaitSettled()
	if err != nil {
		t.Fatalf("WaitSettled failed: %v", err)
	}
	if !report.Settled || report.ReleasedCount != 1 {
		t.Fatalf("Expected 1 released handle, got: %+v", report)
	}
	if !r1.released.Load() {
		t.Fatalf("Resource r1 was not released")
	}
}

// Positive Trace 4: Parent Early Return — defer region.Cancel() triggers clean settlement.
func TestPositiveTrace_ParentEarlyReturn(t *testing.T) {
	r1 := newResource("res-1")

	runScope := func() error {
		region := NewRegion(context.Background())
		defer region.Cancel()

		slot1, err := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })
		if err != nil {
			return err
		}

		region.SpawnTask(func(c context.Context) {
			h, ok := slot1.TryAcquire()
			if !ok {
				return // cancellation won
			}
			select {
			case <-c.Done():
				ReleaseHandle(region, h)
			case <-time.After(100 * time.Millisecond):
				TransferResult(region, h)
			}
		})

		// Simulate parent encountering early error
		return errors.New("parent early exit")
	}

	err := runScope()
	if err == nil || err.Error() != "parent early exit" {
		t.Fatalf("expected early exit error")
	}

	// Give time for deferred cancellation & join
	time.Sleep(20 * time.Millisecond)
	if !r1.released.Load() {
		t.Fatalf("r1 should have been released upon early exit cleanup")
	}
}

// Mutation 1: Child omits release/transfer — region MUST refuse to certify Settled.
func TestMutation_HandleLeakedOmittedCleanup(t *testing.T) {
	region := NewRegion(context.Background())
	r1 := newResource("res-1")

	slot1, _ := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })

	region.SpawnTask(func(c context.Context) {
		_, ok := slot1.TryAcquire()
		if !ok {
			return
		}
		// BUG: Child exits without TransferResult or ReleaseHandle!
	})

	_, err := region.WaitSettled()
	if err == nil {
		t.Fatalf("Expected WaitSettled to fail with ErrIncompleteSettlement, but it succeeded")
	}
	if !errors.Is(err, ErrIncompleteSettlement) {
		t.Fatalf("Expected ErrIncompleteSettlement, got: %v", err)
	}
}

// Mutation 2: Stale / Duplicate Receipt Replay — must be rejected by generation check.
func TestMutation_DuplicateReceiptReplay(t *testing.T) {
	region := NewRegion(context.Background())
	r1 := newResource("res-1")

	slot1, _ := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })
	h1, ok := slot1.TryAcquire()
	if !ok {
		t.Fatalf("failed to acquire")
	}

	_, err := TransferResult(region, h1)
	if err != nil {
		t.Fatalf("first transfer failed: %v", err)
	}

	// Attempt duplicate transfer with same handle
	_, err = TransferResult(region, h1)
	if err == nil {
		t.Fatalf("expected duplicate transfer to fail")
	}
	if !errors.Is(err, ErrDuplicateReceipt) {
		t.Fatalf("expected ErrDuplicateReceipt, got: %v", err)
	}
}

// Mutation 3: Cancellation can NEVER steal child-acquired handle.
func TestMutation_CancellationCannotPreemptAcquiredHandle(t *testing.T) {
	region := NewRegion(context.Background())
	r1 := newResource("res-1")

	slot1, _ := RegisterHandle(region, "h1", r1, func(r *MockResource) { r.release() })
	h1, ok := slot1.TryAcquire()
	if !ok {
		t.Fatalf("failed to acquire")
	}

	// Parent attempts to cancel AFTER child acquired
	region.Cancel()

	// Child still owns the handle; verify it can still transfer or release
	if h1.state != StateAcquired {
		t.Fatalf("handle state corrupted by parent cancel: %v", h1.state)
	}

	_, err := ReleaseHandle(region, h1)
	if err != nil {
		t.Fatalf("child failed to release acquired handle after cancel: %v", err)
	}

	report, err := region.WaitSettled()
	if err != nil {
		t.Fatalf("WaitSettled failed: %v", err)
	}
	if !report.Settled || report.ReleasedCount != 1 {
		t.Fatalf("Expected clean settlement")
	}
}

// Concurrency Stress: 100 concurrent trials of acquisition vs cancellation race.
func TestConcurrency_HandoffRaceIntegrity(t *testing.T) {
	for i := 0; i < 100; i++ {
		region := NewRegion(context.Background())
		res := newResource(string(rune(i)))
		slot, _ := RegisterHandle(region, HandleID(string(rune(i))), res, func(r *MockResource) { r.release() })

		var wg sync.WaitGroup
		wg.Add(2)

		go func() {
			defer wg.Done()
			if h, ok := slot.TryAcquire(); ok {
				ReleaseHandle(region, h)
			}
		}()

		go func() {
			defer wg.Done()
			region.Cancel()
		}()

		wg.Wait()
		report, err := region.WaitSettled()
		if err != nil {
			t.Fatalf("Trial %d failed to settle: %v", i, err)
		}
		if !report.Settled || !res.released.Load() {
			t.Fatalf("Trial %d resource not released or unsettled", i)
		}
	}
}
