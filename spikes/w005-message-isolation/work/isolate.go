// isolate.go — W-005 spike: the boundary helpers emulating what the compiler
// would generate at `task f(args...)` and `ch <- v` sites under
// copy-at-boundary semantics (issue #342, W-005, nikolas-sapa).
//
// Spike contract: the callee must reach mutable state ONLY through its
// parameters — under compiler integration the checker would enforce exactly
// that statically (closure captures of mutable graphs become clone
// candidates too). Here we emulate the generated code shape, not the
// checker.

package iso

import "fmt"

// Spawn clones arg across the task boundary and runs fn on the isolated
// copy in a fresh goroutine. The caller keeps its original; the two graphs
// share nothing mutable. This is the generated-code shape for
// `task f(arg)` under message isolation.
func Spawn[T any](fn func(T), arg T) error {
	anyArg := any(arg)
	c, err := Clone(anyArg)
	if err != nil {
		return fmt.Errorf("iso: task argument cannot be isolated: %w", err)
	}
	casted, ok := c.(T)
	if !ok {
		return fmt.Errorf("iso: isolated argument lost its type (%T -> %T)", arg, c)
	}
	go fn(casted)
	return nil
}

// Send clones v across a channel boundary and delivers the isolated copy.
// The channel itself keeps identity (clone the message, never the pipe).
func Send[T any](ch chan T, v T) error {
	anyV := any(v)
	c, err := Clone(anyV)
	if err != nil {
		return fmt.Errorf("iso: channel send cannot be isolated: %w", err)
	}
	casted, ok := c.(T)
	if !ok {
		return fmt.Errorf("iso: isolated send value lost its type (%T -> %T)", v, c)
	}
	ch <- casted
	return nil
}
