package errgroup

import "context"

// Group preserves the upstream API spelling. The implementation is emitted
// from accepted.hy; these methods only forward to its lowercase methods.
type Group = group

// WithContext stays in Go because context is outside Hybrid's M5 allowlist.
// This is an inventoried escape, not checker-verified cancellation.
func WithContext(ctx context.Context) (*Group, context.Context) {
	ctx, cancel := context.WithCancelCause(ctx)
	return &group{cancel: cancel}, ctx
}

func (g *group) Wait() error               { return g.wait() }
func (g *group) Go(f func() error)         { g.spawn(f) }
func (g *group) TryGo(f func() error) bool { return g.try_spawn(f) }
func (g *group) SetLimit(n int)            { g.set_limit(n) }
