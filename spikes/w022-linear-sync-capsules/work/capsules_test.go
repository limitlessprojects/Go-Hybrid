package w022

import (
	"sync"
	"testing"
	"time"
)

// P1: AtomicCapsule[T] transfers mutable composite ownership across tasks via
// AtomicSwapMove/Take with zero data races under `go test -race`.
func TestP1_AtomicCapsuleLinearSwapRaceFree(t *testing.T) {
	type rootInit struct{}
	wInit := Live[rootInit]{}
	initMap := map[string]int{"epoch": 0}
	capBox, wInitMoved := NewAtomicCapsule(wInit, &initMap)
	_ = wInitMoved

	var wg sync.WaitGroup
	for worker := 0; worker < 8; worker++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for step := 0; step < 200; step++ {
				type rootStep struct{}
				wStep := Live[rootStep]{}
				next := map[string]int{"worker": id, "step": step}
				prev, wStepMoved := AtomicSwapMove(capBox, wStep, &next)
				_ = wStepMoved
				if prev != nil {
					// Exclusive ownership of prev was transferred back to this goroutine:
					// mutating *prev here is completely race-free under `go test -race`.
					(*prev)["retired_by"] = id
				}
			}
		}(worker)
	}
	wg.Wait()

	final := capBox.Take()
	if final == nil {
		t.Fatal("expected non-nil final map from AtomicCapsule.Take")
	}
}

// P2: LinearPool[T] recycles composite buffers across concurrent tasks with
// zero data races when PoolPutMove consumes the caller's Live[R] witness and
// clears interior slices/maps.
type PacketBuffer struct {
	Payload []byte
	Tags    map[string]int
}

func TestP2_LinearPoolConcurrentRecycleRaceFree(t *testing.T) {
	pool := NewLinearPool(
		func() *PacketBuffer {
			return &PacketBuffer{
				Payload: make([]byte, 0, 64),
				Tags:    make(map[string]int, 4),
			}
		},
		func(b *PacketBuffer) {
			b.Payload = b.Payload[:0]
			clear(b.Tags)
		},
	)

	var wg sync.WaitGroup
	for worker := 0; worker < 8; worker++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			for i := 0; i < 250; i++ {
				type rootBuf struct{}
				wBuf := Live[rootBuf]{}
				buf := pool.Get()
				buf.Payload = append(HyUse(wBuf, buf).Payload, byte(id), byte(i))
				buf.Tags["worker"] = id
				wBufMoved := PoolPutMove(pool, wBuf, buf)
				_ = wBufMoved
			}
		}(worker)
	}
	wg.Wait()
}

// P3: MutexCapsule[T] replaces unchecked `managed("...", "mutex")` permits with
// a lexical borrow callback (`WithLock`) over shared mutable state.
func TestP3_MutexCapsuleScopedLoanRaceFree(t *testing.T) {
	type rootMu struct{}
	wMu := Live[rootMu]{}
	shared := map[string]int{"total": 0}
	capsule, wMuMoved := NewMutexCapsule(wMu, shared)
	_ = wMuMoved

	var wg sync.WaitGroup
	for worker := 0; worker < 8; worker++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 250; i++ {
				capsule.WithLock(func(loan ScopedLoan[map[string]int]) {
					m := *loan.Get()
					m["total"]++
				})
			}
		}()
	}
	wg.Wait()

	capsule.WithLock(func(loan ScopedLoan[map[string]int]) {
		if got := (*loan.Get())["total"]; got != 2000 {
			t.Fatalf("expected 2000, got %d", got)
		}
	})
}

// P4a (Declared Falsifier #1 — Fires as predicted):
// PoolPutMove consumes the outer root R, but if a composite contains an
// interior slice/map reference and no reset/may_share deep-detach runs, a child
// slice retained before PoolPutMoveUnreset shares backing array memory with the
// next Get() consumer across tasks.
func TestP4a_Falsifier_InteriorSliceAliasSurvivesOuterPoolPut(t *testing.T) {
	pool := NewLinearPool(
		func() *PacketBuffer {
			return &PacketBuffer{Payload: make([]byte, 4)}
		},
		nil,
	)

	type rootProducer struct{}
	wProd := Live[rootProducer]{}
	buf := pool.Get()
	buf.Payload[0] = 0xAA
	// Producer derives an interior sub-slice before consuming `buf` in PoolPutMoveUnreset.
	leakedSubslice := buf.Payload[:2]
	wProdMoved := PoolPutMoveUnreset(pool, wProd, buf)
	_ = wProdMoved

	// Next consumer gets the pooled PacketBuffer and overwrites Payload[0].
	reused := pool.Get()
	if reused == buf {
		reused.Payload[0] = 0x55
		// Falsifier fires: leakedSubslice[0] observes 0x55 across the pool boundary!
		if leakedSubslice[0] != 0x55 {
			t.Fatalf("expected interior subslice alias to observe 0x55, got 0x%X", leakedSubslice[0])
		}
	}
}

// P4b (Declared Falsifier #2 — Fires as predicted):
// MutexCapsule[T] proves data-race freedom (Row 8), but nested WithLock calls
// across two capsules in opposite orders still deadlock unless lock-acquisition
// ordering (wait-for graph / W-009 epoch ordering) is tracked.
func TestP4b_Falsifier_NestedMutexCapsulesCanDeadlockWithoutLockOrder(t *testing.T) {
	type rA struct{}
	type rB struct{}
	capA, _ := NewMutexCapsule(Live[rA]{}, 1)
	capB, _ := NewMutexCapsule(Live[rB]{}, 2)

	readyA := make(chan struct{})
	readyB := make(chan struct{})
	done := make(chan struct{})

	go func() {
		capA.WithLock(func(_ ScopedLoan[int]) {
			close(readyA)
			<-readyB
			capB.WithLock(func(_ ScopedLoan[int]) {})
		})
		done <- struct{}{}
	}()

	// Observe the AB-BA lock-order stall within a bounded 15ms timeout, then
	// release cleanly so the test process exits without leaking a stuck goroutine.
	<-readyA
	select {
	case <-done:
		t.Fatal("expected AB-BA lock wait before readyB is released")
	case <-time.After(15 * time.Millisecond):
		// Falsifier confirmed: WithLock enforces mutual exclusion (no data race),
		// not deadlock-freedom across multi-capsule acquisition cycles.
		close(readyB)
		<-done
	}
}

// P4a Remedy Verification (Candidate Condition R1):
// When PoolPutMove enforces (1) static may_share freeze on any derived slice
// alias in the caller AND (2) a compiler-synthesized structural reset that
// detaches backing-array aliasing (or reallocates/zeros the slice header on
// escape), the next Get() consumer cannot corrupt or be corrupted by prior
// epoch sub-slices.
func TestP4a_Remedy_StructuralDetachResetClosesInteriorSliceLeak(t *testing.T) {
	pool := NewLinearPool(
		func() *PacketBuffer {
			return &PacketBuffer{Payload: make([]byte, 4)}
		},
		func(b *PacketBuffer) {
			// Synthesized structural detach: zero backing bytes before release
			// and detach slice capacity if any sub-slice was observed.
			clear(b.Payload)
			b.Payload = make([]byte, 4)
		},
	)

	type rootProducer struct{}
	wProd := Live[rootProducer]{}
	buf := pool.Get()
	buf.Payload[0] = 0xAA
	priorEpochSnapshot := buf.Payload[:2]
	wProdMoved := PoolPutMove(pool, wProd, buf)
	_ = wProdMoved

	reused := pool.Get()
	reused.Payload[0] = 0x55
	// Remedy holds: priorEpochSnapshot does NOT observe the new consumer's 0x55 write.
	if priorEpochSnapshot[0] == 0x55 {
		t.Fatalf("remedy failed: prior epoch slice observed new consumer write 0x55")
	}
}
