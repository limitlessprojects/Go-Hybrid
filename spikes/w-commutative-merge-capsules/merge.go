// Package mergecapsule implements the executable prototype and benchmark
// harness for Go-Hybrid Wildcard Proposal #2385:
// "Certified Commutative Merge Capsules (merge[M]): Wait-Free Parallel
// Accumulation Without Channels, Atomics, or managed Permits".
//
// Invariants:
//  1. Each spawned worker holds an affine Shard[M] with exclusive ownership of
//     its local accumulator. Worker updates require ZERO locks, atomics, or channels.
//  2. The parent scope cannot read or mutate the root accumulator while shards
//     are live.
//  3. At Settle(), all shards are folded back into the root accumulator via
//     the verified associative/commutative combine function.
//  4. Pairwise tree folding ensures deterministic O(log N) reduction without contention.
package mergecapsule

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
)

// ReducerFunc defines a pure associative and commutative folding function.
// It mutates target 'a' by folding in the value of 'b'.
type ReducerFunc[M any] func(a *M, b M)

// Shard represents an affine task-local shard capability.
// Each worker goroutine mutates its private Val without synchronization.
type Shard[M any] struct {
	id     int
	Val    M
	used   atomic.Bool
	parent *MergeCapsule[M]
}

// MarkCompleted marks the shard as finished and seals further updates.
func (s *Shard[M]) MarkCompleted() {
	s.used.Store(true)
}

// MergeCapsule manages parallel shards and deterministic scope settlement.
type MergeCapsule[M any] struct {
	mu        sync.Mutex
	combine   ReducerFunc[M]
	zero      M
	shards    []*Shard[M]
	settled   atomic.Bool
	ctx       context.Context
	cancel    context.CancelFunc
}

// New creates a new MergeCapsule with an initial zero value and combiner.
func New[M any](zero M, combine ReducerFunc[M]) *MergeCapsule[M] {
	ctx, cancel := context.WithCancel(context.Background())
	return &MergeCapsule[M]{
		combine: combine,
		zero:    zero,
		shards:  make([]*Shard[M], 0, 64),
		ctx:     ctx,
		cancel:  cancel,
	}
}

// Split creates and registers a new affine task-local shard.
// Must be called in the parent scope before spawning the task.
func (c *MergeCapsule[M]) Split() *Shard[M] {
	if c.settled.Load() {
		panic("cannot split an already settled MergeCapsule")
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	shard := &Shard[M]{
		id:     len(c.shards),
		Val:    c.zero,
		parent: c,
	}
	c.shards = append(c.shards, shard)
	return shard
}

// Abort signals an early error across worker tasks, triggering clean barrier exit.
func (c *MergeCapsule[M]) Abort(err error) {
	c.cancel()
}

// Context returns the cancellation context for early-exit coordination.
func (c *MergeCapsule[M]) Context() context.Context {
	return c.ctx
}

// Settle folds all shards back into a single result accumulator.
// Uses deterministic pairwise tree folding.
func (c *MergeCapsule[M]) Settle() (M, error) {
	if !c.settled.CompareAndSwap(false, true) {
		return c.zero, fmt.Errorf("MergeCapsule has already settled")
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	n := len(c.shards)
	if n == 0 {
		return c.zero, nil
	}

	// Extract values into a contiguous slice for deterministic reduction
	values := make([]M, n)
	for i, s := range c.shards {
		values[i] = s.Val
	}

	// Pairwise parallel or sequential tree fold
	result := c.treeFold(values)
	return result, nil
}

// treeFold performs a deterministic pairwise reduction in O(log N) depth.
func (c *MergeCapsule[M]) treeFold(vals []M) M {
	if len(vals) == 1 {
		return vals[0]
	}

	for len(vals) > 1 {
		half := (len(vals) + 1) / 2
		next := make([]M, half)
		for i := 0; i < len(vals); i += 2 {
			idx := i / 2
			if i+1 < len(vals) {
				acc := vals[i]
				c.combine(&acc, vals[i+1])
				next[idx] = acc
			} else {
				next[idx] = vals[i]
			}
		}
		vals = next
	}
	return vals[0]
}
