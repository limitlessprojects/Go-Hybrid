package work

import (
	"context"
	"sync"
	"sync/atomic"
)

// TaskNursery coordinates structured concurrent tasks and enforces complete teardown before exit.
type TaskNursery struct {
	ctx     context.Context
	cancel  context.CancelFunc
	wg      sync.WaitGroup
	active  int64
	drained int64
}

// NewTaskNursery creates a new task nursery tied to an epoch context.
func NewTaskNursery(parent context.Context) *TaskNursery {
	ctx, cancel := context.WithCancel(parent)
	return &TaskNursery{
		ctx:    ctx,
		cancel: cancel,
	}
}

// Context returns the nursery's cancellation context.
func (n *TaskNursery) Context() context.Context {
	return n.ctx
}

// Cancel signals cooperative cancellation to all tasks in the nursery.
func (n *TaskNursery) Cancel() {
	n.cancel()
}

// Spawn registers a task with an affine CancelWitness token.
func (n *TaskNursery) Spawn(fn func(ctx context.Context, w *CancelWitness)) {
	n.wg.Add(1)
	atomic.AddInt64(&n.active, 1)
	w := &CancelWitness{nursery: n}
	go func() {
		defer func() {
			if !w.discharged {
				w.Complete()
			}
		}()
		fn(n.ctx, w)
	}()
}

// Wait blocks until all spawned tasks have cleanly discharged their teardown obligations.
func (n *TaskNursery) Wait() {
	n.wg.Wait()
}

// ActiveCount returns current active tasks in the nursery.
func (n *TaskNursery) ActiveCount() int64 {
	return atomic.LoadInt64(&n.active)
}

// DrainedCount returns tasks that completed via cooperative abort/drain.
func (n *TaskNursery) DrainedCount() int64 {
	return atomic.LoadInt64(&n.drained)
}

// CancelWitness is an affine capability token ensuring each task reports completion or drain.
type CancelWitness struct {
	nursery    *TaskNursery
	discharged bool
}

// Complete discharges the witness upon successful task completion.
func (w *CancelWitness) Complete() {
	if !w.discharged {
		w.discharged = true
		atomic.AddInt64(&w.nursery.active, -1)
		w.nursery.wg.Done()
	}
}

// Abort discharges the witness upon cooperative cancellation and channel drain.
func (w *CancelWitness) Abort() {
	if !w.discharged {
		w.discharged = true
		atomic.AddInt64(&w.nursery.active, -1)
		atomic.AddInt64(&w.nursery.drained, 1)
		w.nursery.wg.Done()
	}
}

// CooperativeChannel provides an affine drain interface for cancellation-safe communication.
type CooperativeChannel[T any] struct {
	ch     chan T
	closed atomic.Bool
}

// NewCooperativeChannel creates a channel with bounded buffer capacity.
func NewCooperativeChannel[T any](cap int) *CooperativeChannel[T] {
	return &CooperativeChannel[T]{
		ch: make(chan T, cap),
	}
}

// Send attempts to send an item; returns false if the channel or context is terminated.
func (c *CooperativeChannel[T]) Send(ctx context.Context, val T) bool {
	select {
	case <-ctx.Done():
		return false
	case c.ch <- val:
		return true
	}
}

// Recv receives an item; returns (val, true) on item, (zero, false) on cancel/close.
func (c *CooperativeChannel[T]) Recv(ctx context.Context) (T, bool) {
	select {
	case <-ctx.Done():
		var zero T
		return zero, false
	case val, ok := <-c.ch:
		return val, ok
	}
}

// Drain non-blockingly empties any stranded items in the channel to prevent producer deadlocks.
func (c *CooperativeChannel[T]) Drain() int {
	drained := 0
	for {
		select {
		case _, ok := <-c.ch:
			if !ok {
				return drained
			}
			drained++
		default:
			return drained
		}
	}
}

// Close marks the channel closed.
func (c *CooperativeChannel[T]) Close() {
	if c.closed.CompareAndSwap(false, true) {
		close(c.ch)
	}
}
