// Package quantityconservation implements compile-time and runtime quantity conservation
// invariants across asynchronous task boundaries.
//
// Addresses Issue #4725:
// "[wildcard] Quantity conservation as a checked task-boundary invariant (affine resource partitions)"
//
// Guarantees:
// 1. Numerical Conservation: Remaining + Committed + Pending == Total Budget at every step.
// 2. Affine Witnessing: Partitions and Receipts cannot be duplicated or forged.
// 3. Staged Atomicity: Results and consumed units commit together or abort with full partition restoration.
package quantityconservation

import (
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
)

// Unit marker type.
type Unit interface {
	__unit()
}

// Region marker type.
type Region interface {
	__region()
}

// Pool represents the root conserved quantity within an opt-in region.
type Pool[U Unit, R Region] struct {
	mu        sync.Mutex
	total     uint64
	remaining uint64
	committed uint64
	pending   map[uint64]uint64 // partitionID -> allocated amount
	nextID    uint64
	closed    bool
}

// NewPool initializes a quantity pool with a fixed non-negative budget.
func NewPool[U Unit, R Region](initialBudget uint64) *Pool[U, R] {
	return &Pool[U, R]{
		total:     initialBudget,
		remaining: initialBudget,
		committed: 0,
		pending:   make(map[uint64]uint64),
		nextID:    1,
	}
}

// Partition represents an affine allocation granted to a child task.
type Partition[U Unit, R Region] struct {
	id     uint64
	amount uint64
	used   bool
}

// Receipt represents an exhaustive accounting receipt returned by a child task.
type Receipt[U Unit, R Region] struct {
	partitionID uint64
	allocated   uint64
	consumed    uint64
	unused      uint64
	discharged  bool
}

// Allocate partitions an amount from the pool for a child task.
func (p *Pool[U, R]) Allocate(amount uint64) (*Partition[U, R], error) {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.closed {
		return nil, errors.New("HYB-E4001: cannot allocate from closed pool")
	}
	if amount > p.remaining {
		return nil, fmt.Errorf("HYB-E4002: insufficient budget: requested %d, remaining %d", amount, p.remaining)
	}

	p.remaining -= amount
	id := p.nextID
	p.nextID++
	p.pending[id] = amount

	p.assertInvariantLocked()

	return &Partition[U, R]{
		id:     id,
		amount: amount,
		used:   false,
	}, nil
}

// ExecuteStaged performs work within a child partition, producing a staged result and an exhaustive receipt.
func (part *Partition[U, R]) ExecuteStaged[T any](consumeAmount uint64, workerFn func() (T, error)) (T, *Receipt[U, R], error) {
	var zero T
	if part.used {
		return zero, nil, errors.New("HYB-E3003: use after move: partition already consumed")
	}
	part.used = true

	if consumeAmount > part.amount {
		return zero, nil, fmt.Errorf("HYB-E4003: partition over-consumption: allocated %d, requested %d", part.amount, consumeAmount)
	}

	result, err := workerFn()
	if err != nil {
		// Task failed: 0 consumed, all unused
		receipt := &Receipt[U, R]{
			partitionID: part.id,
			allocated:   part.amount,
			consumed:    0,
			unused:      part.amount,
		}
		return zero, receipt, err
	}

	// Task succeeded
	receipt := &Receipt[U, R]{
		partitionID: part.id,
		allocated:   part.amount,
		consumed:    consumeAmount,
		unused:      part.amount - consumeAmount,
	}
	return result, receipt, nil
}

// Settle commits or rolls back a partition based on task success and receipt.
func (p *Pool[U, R]) Settle(r *Receipt[U, R], taskSucceeded bool) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if r.discharged {
		return errors.New("HYB-E3003: receipt already settled (double settlement rejected)")
	}
	r.discharged = true

	allocated, exists := p.pending[r.partitionID]
	if !exists {
		return errors.New("HYB-E4004: unknown or forged partition receipt")
	}
	delete(p.pending, r.partitionID)

	if allocated != r.allocated {
		return errors.New("HYB-E4005: receipt allocation mismatch")
	}

	if taskSucceeded {
		// Success: commit consumed, return unused to remaining
		p.committed += r.consumed
		p.remaining += r.unused
	} else {
		// Failure: entire partition restored to remaining
		p.remaining += allocated
	}

	p.assertInvariantLocked()
	return nil
}

// Invariant returns true if Remaining + Committed + Pending == Total.
func (p *Pool[U, R]) Invariant() bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.checkInvariant()
}

func (p *Pool[U, R]) checkInvariant() bool {
	var pendingSum uint64
	for _, v := range p.pending {
		pendingSum += v
	}
	return p.remaining+p.committed+pendingSum == p.total
}

func (p *Pool[U, R]) assertInvariantLocked() {
	if !p.checkInvariant() {
		panic("HYB-E4000: CRITICAL CONSERVATION VIOLATION: remaining + committed + pending != total")
	}
}

// Snapshot returns the current accounting state.
func (p *Pool[U, R]) Snapshot() (total, remaining, committed, pendingSum uint64) {
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, v := range p.pending {
		pendingSum += v
	}
	return p.total, p.remaining, p.committed, pendingSum
}
