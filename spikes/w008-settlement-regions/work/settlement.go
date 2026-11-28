// Package w008 implements the W-008 spike: checked settlement for own-handle task regions.
// Proposal: issue #257 (luvskyy, "Cancellation is not completion: checked settlement for own-handle task regions").
//
// Core Invariants:
// 1. Completion is an ownership boundary: a region may return Settled only when EVERY registered
//    child has been joined AND every registered own-handle has reached exactly one verified
//    terminal outcome: ownership transferred into its returned result bundle, or synchronous release completed.
// 2. Cancellation is request-only: cancellation never preempts or forcibly reclaims a child-acquired
//    handle. If acquisition wins, only the child can settle the handle. If cancellation wins the
//    atomic handoff before acquisition, the parent ledger assumes sole responsibility for synchronous release.
// 3. Affine terminal accounting: receipts are single-use, generation-pinned proofs of transfer or release.
//    Missing, duplicate, or stale receipts fail closed before Settled is certified.
package w008

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
	"time"
)

var (
	ErrHandleAlreadyRegistered = errors.New("w008: handle already registered in region")
	ErrHandleNotRegistered     = errors.New("w008: handle not registered in region")
	ErrHandleAlreadySettled    = errors.New("w008: handle has already reached terminal settlement")
	ErrHandleNotAcquired       = errors.New("w008: operation requires acquired handle ownership")
	ErrIncompleteSettlement    = errors.New("w008: region cannot settle: one or more handles remain unsettled")
	ErrDuplicateReceipt        = errors.New("w008: duplicate or stale receipt detected")
	ErrAcquisitionCancelled    = errors.New("w008: handoff acquisition rejected: cancelled by region")
	ErrRegionClosed            = errors.New("w008: region is closed for new handle registrations")
)

// HandleState models the affine lifecycle of an owned resource handle.
type HandleState int

const (
	StateUnregistered HandleState = iota
	StateEscrowed                 // Registered in parent ledger; waiting for child acquisition
	StateAcquired                 // Acquired by child task; child holds sole ownership
	StateTransferred              // Transferred into result bundle (terminal outcome 1)
	StateReleased                 // Synchronously released / cleaned up (terminal outcome 2)
)

func (s HandleState) String() string {
	switch s {
	case StateUnregistered:
		return "Unregistered"
	case StateEscrowed:
		return "Escrowed"
	case StateAcquired:
		return "Acquired"
	case StateTransferred:
		return "Transferred"
	case StateReleased:
		return "Released"
	default:
		return fmt.Sprintf("Unknown(%d)", s)
	}
}

// HandleID identifies a unique handle within a region.
type HandleID string

// Handle represents an owned resource governed by affine settlement.
type Handle[T any] struct {
	id         HandleID
	val        T
	generation uint64
	state      HandleState
	releaser   func(T)
}

// ID returns the handle identifier.
func (h *Handle[T]) ID() HandleID { return h.id }

// Value returns a reference to the contained value if in Acquired state.
func (h *Handle[T]) Value() (T, error) {
	if h.state != StateAcquired {
		var zero T
		return zero, ErrHandleNotAcquired
	}
	return h.val, nil
}

// Receipt is an affine proof that a handle has reached a terminal outcome.
type Receipt struct {
	HandleID      HandleID
	Generation    uint64
	TerminalState HandleState
	Timestamp     time.Time
}

// HandoffState values for atomic CAS transitions.
const (
	HandoffPending   uint32 = 0
	HandoffAcquired  uint32 = 1
	HandoffCancelled uint32 = 2
)

// HandoffSlot coordinates the race between child acquisition and parent cancellation.
type HandoffSlot[T any] struct {
	handle *Handle[T]
	state  atomic.Uint32
}

// TryAcquire attempts child acquisition.
// Returns (handle, true) if child won the race.
// Returns (nil, false) if parent cancellation won.
func (s *HandoffSlot[T]) TryAcquire() (*Handle[T], bool) {
	if s.state.CompareAndSwap(HandoffPending, HandoffAcquired) {
		s.handle.state = StateAcquired
		return s.handle, true
	}
	return nil, false
}

// tryCancel attempts parent cancellation before acquisition.
// Returns true if cancellation won and child can never acquire.
func (s *HandoffSlot[T]) tryCancel() bool {
	return s.state.CompareAndSwap(HandoffPending, HandoffCancelled)
}

type trackedEntry struct {
	id         HandleID
	generation uint64
	state      HandleState
	releaseFn  func()
	slotCancel func() bool
}

// Region enforces checked settlement across a finite child set.
type Region struct {
	ctx        context.Context
	cancel     context.CancelFunc
	wg         sync.WaitGroup
	mu         sync.Mutex
	genCounter uint64
	handles    map[HandleID]*trackedEntry
	receipts   map[HandleID]Receipt
	closed     bool
}

// NewRegion constructs a settlement region.
func NewRegion(parentCtx context.Context) *Region {
	ctx, cancel := context.WithCancel(parentCtx)
	return &Region{
		ctx:      ctx,
		cancel:   cancel,
		handles:  make(map[HandleID]*trackedEntry),
		receipts: make(map[HandleID]Receipt),
	}
}

// Context returns the region's cooperative cancellation context.
func (r *Region) Context() context.Context {
	return r.ctx
}

// RegisterHandle registers an own handle into the region's escrow ledger.
func RegisterHandle[T any](r *Region, id HandleID, val T, releaser func(T)) (*HandoffSlot[T], error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.closed {
		return nil, ErrRegionClosed
	}
	if _, exists := r.handles[id]; exists {
		return nil, ErrHandleAlreadyRegistered
	}

	r.genCounter++
	gen := r.genCounter

	handle := &Handle[T]{
		id:         id,
		val:        val,
		generation: gen,
		state:      StateEscrowed,
		releaser:   releaser,
	}

	slot := &HandoffSlot[T]{
		handle: handle,
	}

	r.handles[id] = &trackedEntry{
		id:         id,
		generation: gen,
		state:      StateEscrowed,
		releaseFn: func() {
			if releaser != nil {
				releaser(val)
			}
		},
		slotCancel: slot.tryCancel,
	}

	return slot, nil
}

// SpawnTask registers a child task in the region's join group.
func (r *Region) SpawnTask(fn func(ctx context.Context)) {
	r.wg.Add(1)
	go func() {
		defer r.wg.Done()
		fn(r.ctx)
	}()
}

// TransferResult registers terminal outcome: handle ownership transferred to result bundle.
func TransferResult[T any](r *Region, h *Handle[T]) (Receipt, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	entry, ok := r.handles[h.id]
	if !ok || entry.generation != h.generation {
		return Receipt{}, ErrHandleNotRegistered
	}
	if h.state != StateAcquired {
		return Receipt{}, ErrHandleNotAcquired
	}
	if _, already := r.receipts[h.id]; already {
		return Receipt{}, ErrDuplicateReceipt
	}

	h.state = StateTransferred
	entry.state = StateTransferred

	receipt := Receipt{
		HandleID:      h.id,
		Generation:    h.generation,
		TerminalState: StateTransferred,
		Timestamp:     time.Now().UTC(),
	}
	r.receipts[h.id] = receipt
	return receipt, nil
}

// ReleaseHandle registers terminal outcome: handle synchronously released by child.
func ReleaseHandle[T any](r *Region, h *Handle[T]) (Receipt, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	entry, ok := r.handles[h.id]
	if !ok || entry.generation != h.generation {
		return Receipt{}, ErrHandleNotRegistered
	}
	if h.state != StateAcquired {
		return Receipt{}, ErrHandleNotAcquired
	}
	if _, already := r.receipts[h.id]; already {
		return Receipt{}, ErrDuplicateReceipt
	}

	if h.releaser != nil {
		h.releaser(h.val)
	}

	h.state = StateReleased
	entry.state = StateReleased

	receipt := Receipt{
		HandleID:      h.id,
		Generation:    h.generation,
		TerminalState: StateReleased,
		Timestamp:     time.Now().UTC(),
	}
	r.receipts[h.id] = receipt
	return receipt, nil
}

// Cancel requests cooperative settlement across all children and reclaims unacquired handoffs.
func (r *Region) Cancel() {
	r.mu.Lock()
	defer r.mu.Unlock()

	r.cancel()

	// For every handle still in Escrowed state, attempt atomic cancellation before child acquisition.
	for id, entry := range r.handles {
		if entry.state == StateEscrowed && entry.slotCancel != nil {
			if entry.slotCancel() {
				// Cancellation won before child acquisition. Parent ledger releases synchronously.
				if entry.releaseFn != nil {
					entry.releaseFn()
				}
				entry.state = StateReleased
				r.receipts[id] = Receipt{
					HandleID:      id,
					Generation:    entry.generation,
					TerminalState: StateReleased,
					Timestamp:     time.Now().UTC(),
				}
			}
		}
	}
}

// SettlementReport summarizes the terminal outcomes of a settled region.
type SettlementReport struct {
	Settled         bool
	TotalHandles    int
	TransferredCount int
	ReleasedCount   int
	Receipts        map[HandleID]Receipt
}

// WaitSettled joins all registered children and validates terminal outcome accounting.
// Returns ErrIncompleteSettlement if any handle lacks a terminal receipt.
// Blocks indefinitely if any child fails to exit (Cancellation is NOT completion).
func (r *Region) WaitSettled() (SettlementReport, error) {
	r.mu.Lock()
	r.closed = true
	r.mu.Unlock()

	// 1. Join all child tasks. If a child hangs, this correctly blocks.
	r.wg.Wait()

	r.mu.Lock()
	defer r.mu.Unlock()

	report := SettlementReport{
		TotalHandles: len(r.handles),
		Receipts:     make(map[HandleID]Receipt),
	}

	for id, entry := range r.handles {
		receipt, ok := r.receipts[id]
		if !ok {
			return report, fmt.Errorf("%w: handle %s has no terminal receipt (state=%s)", ErrIncompleteSettlement, id, entry.state)
		}
		if receipt.TerminalState != StateTransferred && receipt.TerminalState != StateReleased {
			return report, fmt.Errorf("%w: handle %s ended in non-terminal state %s", ErrIncompleteSettlement, id, receipt.TerminalState)
		}
		if receipt.Generation != entry.generation {
			return report, fmt.Errorf("%w: handle %s generation mismatch", ErrDuplicateReceipt, id)
		}

		if receipt.TerminalState == StateTransferred {
			report.TransferredCount++
		} else {
			report.ReleasedCount++
		}
		report.Receipts[id] = receipt
	}

	report.Settled = true
	return report, nil
}
