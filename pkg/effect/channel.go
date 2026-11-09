package effect

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
)

var (
	// ErrChannelClosed indicates an attempt to send on a closed typestated channel.
	ErrChannelClosed = errors.New("effect: send on closed channel")
	// ErrNurseryCanceled indicates channel operation aborted due to nursery cancellation.
	ErrNurseryCanceled = errors.New("effect: nursery context canceled")
	// ErrAlreadyClosed indicates repeated close call on an endpoint.
	ErrAlreadyClosed = errors.New("effect: channel endpoint already closed")
)

// ChannelState represents the typestated lifecycle of a channel endpoint.
type ChannelState int32

const (
	StateOpen ChannelState = iota
	StateClosed
)

type channelState[T any] struct {
	ch     chan T
	state  int32
	once   sync.Once
	ctx    context.Context
	cancel context.CancelFunc
}

// TxChannel represents a send-capable channel endpoint bound to an effect context.
type TxChannel[T any] struct {
	inner *channelState[T]
}

// RxChannel represents a receive-capable channel endpoint bound to an effect context.
type RxChannel[T any] struct {
	inner *channelState[T]
}

// NewChannel creates a typestated channel pair bound to a nursery or context boundary.
func NewChannel[T any](ctx context.Context, buffer int) (*TxChannel[T], *RxChannel[T]) {
	cCtx, cancel := context.WithCancel(ctx)
	inner := &channelState[T]{
		ch:     make(chan T, buffer),
		state:  int32(StateOpen),
		ctx:    cCtx,
		cancel: cancel,
	}
	return &TxChannel[T]{inner: inner}, &RxChannel[T]{inner: inner}
}

// Send pushes a item into the channel while observing call and nursery context cancellation.
func (tx *TxChannel[T]) Send(ctx context.Context, val T) error {
	if atomic.LoadInt32(&tx.inner.state) == int32(StateClosed) {
		return ErrChannelClosed
	}

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-tx.inner.ctx.Done():
		return ErrNurseryCanceled
	case tx.inner.ch <- val:
		return nil
	}
}

// Close safely transitions the channel state to closed and notifies receivers.
func (tx *TxChannel[T]) Close() error {
	if !atomic.CompareAndSwapInt32(&tx.inner.state, int32(StateOpen), int32(StateClosed)) {
		return ErrAlreadyClosed
	}
	tx.inner.once.Do(func() {
		close(tx.inner.ch)
		tx.inner.cancel()
	})
	return nil
}

// State returns current channel lifecycle state.
func (tx *TxChannel[T]) State() ChannelState {
	return ChannelState(atomic.LoadInt32(&tx.inner.state))
}

// Recv receives a payload from the channel endpoint.
func (rx *RxChannel[T]) Recv(ctx context.Context) (T, bool, error) {
	var zero T
	select {
	case <-ctx.Done():
		return zero, false, ctx.Err()
	case val, ok := <-rx.inner.ch:
		return val, ok, nil
	}
}

// Nursery manages structured concurrency tasks with deterministic cancellation propagation.
type Nursery struct {
	ctx     context.Context
	cancel  context.CancelFunc
	wg      sync.WaitGroup
	errOnce sync.Once
	err     error
}

// NewNursery initializes a structured scope.
func NewNursery(parent context.Context) (*Nursery, context.Context) {
	ctx, cancel := context.WithCancel(parent)
	return &Nursery{
		ctx:    ctx,
		cancel: cancel,
	}, ctx
}

// Start spawns a task inside the nursery.
func (n *Nursery) Start(fn func(ctx context.Context) error) {
	n.wg.Add(1)
	go func() {
		defer n.wg.Done()
		if err := fn(n.ctx); err != nil {
			n.errOnce.Do(func() {
				n.err = err
				n.cancel()
			})
		}
	}()
}

// Wait blocks until all spawned nursery tasks complete or fail.
func (n *Nursery) Wait() error {
	n.wg.Wait()
	n.cancel()
	return n.err
}
