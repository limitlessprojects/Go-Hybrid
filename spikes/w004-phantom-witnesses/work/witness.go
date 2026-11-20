// W-004 spike: phantom capability witnesses — the zero-cost encoding the
// compiler would synthesize if checker verdicts were carried into emitted Go
// as TYPES instead of runtime checks. Preregistered in ../RESULTS.md.
//
// Encoding contract (what `hyb emit` would synthesize per binding):
//   - one unique zero-size root marker type per binding: `type hyR_x struct{}`
//   - the binding's STATE lives in a witness variable of type live[R] / moved[R]
//   - every USE of the binding goes through hyUse (the emitter emits it; the
//     Go compiler's type checker enforces it at ZERO runtime cost)
//   - hyMove consumes the live witness and returns the moved witness plus the
//     value, shadowing BOTH in the continuation scope (the emitter emits the
//     shadowing block; the source-level `move x` discipline is what the
//     checker enforces — the emitted shape is only ever machine-written)
package w004

// Live is the witness that a binding of root marker R is usable.
type Live[R any] struct{}

// Moved is the witness that a binding of root marker R was moved away.
type Moved[R any] struct{}

// hyUse is the use guard: only a LIVE witness lets the value out.
// Zero-cost: struct{} argument, inlined, no allocation.
func hyUse[R any, T any](w Live[R], v T) T { return v }

// hyMove transfers ownership: consumes the live witness, hands back the moved
// witness and the value. The caller shadows (w, x) with the results in the
// lexical continuation; every later hyUse on the shadowed scope must name the
// moved witness, which no hyUse accepts.
func hyMove[R any, T any](w Live[R], v T) (Moved[R], T) { return Moved[R]{}, v }

// hyClone is the W-005 message-isolation seam (kept so the two spikes can
// share the boundary vocabulary): a boundary receives a deep copy and a FRESH
// live witness — clones restart the ownership story for the far side.
func hyClone[R any, T any](v T, deep func(T) T) (Live[R], T) { return Live[R]{}, deep(v) }
