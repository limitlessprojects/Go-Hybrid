// clone.go — W-005 message-isolation spike: memoized, type-directed deep
// clone for the supported reachable object graph.
//
// Contract (the bounded cloneability contract from issue #342):
//
//   - Clone(v) returns a deep copy in which every mutable GC value reachable
//     from the copy is freshly allocated INSIDE the copy. The sender keeps
//     its original; the receiver's writes cannot touch it, and vice versa.
//   - The memo table preserves identity relationships WITHIN the source
//     graph: two names aliasing one node at the boundary alias the copy;
//     cycles come back as cycles. The memo key is the REFERENCED object's
//     identity — (type, pointer) for maps/pointers/chans, (type, pointer,
//     len, cap) for slices so distinct views of one backing array stay
//     distinct (same-header views unify).
//   - Structs and arrays have no identity in Go (copied by value) — they
//     recurse without memoization; aliasing through them exists only via
//     pointers, which the memo covers.
//   - Channels copy by IDENTITY: a chan is a synchronization object, not
//     graph state — clone the message, never the pipe.
//   - Scalars (bool/int*/uint*/float*/complex*/string) copy by value: they
//     are immutable, sharing is unobservable.
//   - FAIL-CLOSED: func values and uintptr are rejected with an error, and
//     any struct with unexported fields is rejected (reflection cannot set
//     them without unsafe). Nothing silently falls back to a shared
//     reference — a rejected value must surface at the boundary, not race
//     in production.
//
// This is the spike shim for what the compiler would generate at task/send
// sites under copy-at-boundary semantics (issue #342, W-005, nikolas-sapa).

package iso

import (
	"fmt"
	"reflect"
)

// Clone deep-copies v under the contract above.
func Clone(v any) (any, error) {
	c, err := cloneValue(reflect.ValueOf(v), map[memoKey]reflect.Value{})
	if err != nil {
		return nil, err
	}
	return c.Interface(), nil
}

// memoKey identifies the REFERENCED object, not the reflect.Value slot:
// the same map reached through a struct field and through an interface box
// must unify (that is the alias-preservation promise).
type memoKey struct {
	typ reflect.Type
	ptr uintptr
	ln  int64
	cp  int64
}

func keyOf(v reflect.Value) memoKey {
	if v.Kind() == reflect.Slice {
		// Distinct views of one backing array are distinct objects for
		// isolation purposes; identical headers unify.
		return memoKey{v.Type(), v.Pointer(), int64(v.Len()), int64(v.Cap())}
	}
	return memoKey{v.Type(), v.Pointer(), 0, 0}
}

var scalarKinds = map[reflect.Kind]bool{
	reflect.Bool: true, reflect.Int: true, reflect.Int8: true,
	reflect.Int16: true, reflect.Int32: true, reflect.Int64: true,
	reflect.Uint: true, reflect.Uint8: true, reflect.Uint16: true,
	reflect.Uint32: true, reflect.Uint64: true,
	reflect.Float32: true, reflect.Float64: true,
	reflect.Complex64: true, reflect.Complex128: true,
	reflect.String: true,
}

func cloneValue(v reflect.Value, memo map[memoKey]reflect.Value) (reflect.Value, error) {
	if !v.IsValid() {
		return v, nil
	}
	switch v.Kind() {
	case reflect.Chan:
		// Channels keep identity: same pipe on both sides.
		return v, nil
	case reflect.Pointer:
		if v.IsNil() {
			return v, nil
		}
		k := keyOf(v)
		if c, ok := memo[k]; ok {
			return c, nil
		}
		cp := reflect.New(v.Type().Elem())
		memo[k] = cp
		if err := cloneInto(cp.Elem(), v.Elem(), memo); err != nil {
			return reflect.Value{}, err
		}
		return cp, nil
	case reflect.Map:
		if v.IsNil() {
			return v, nil
		}
		k := keyOf(v)
		if c, ok := memo[k]; ok {
			return c, nil
		}
		cp := reflect.MakeMapWithSize(v.Type(), v.Len())
		memo[k] = cp
		iter := v.MapRange()
		for iter.Next() {
			kk, err := cloneValue(iter.Key(), memo)
			if err != nil {
				return reflect.Value{}, err
			}
			val, err := cloneValue(iter.Value(), memo)
			if err != nil {
				return reflect.Value{}, err
			}
			cp.SetMapIndex(kk, val)
		}
		return cp, nil
	case reflect.Slice:
		if v.IsNil() {
			return v, nil
		}
		k := keyOf(v)
		if c, ok := memo[k]; ok {
			return c, nil
		}
		cp := reflect.MakeSlice(v.Type(), v.Len(), v.Cap())
		memo[k] = cp
		for i := 0; i < v.Len(); i++ {
			ev, err := cloneValue(v.Index(i), memo)
			if err != nil {
				return reflect.Value{}, err
			}
			cp.Index(i).Set(ev)
		}
		return cp, nil
	case reflect.Array:
		// No memo: arrays are values, no identity to preserve.
		cp := reflect.New(v.Type()).Elem()
		for i := 0; i < v.Len(); i++ {
			ev, err := cloneValue(v.Index(i), memo)
			if err != nil {
				return reflect.Value{}, err
			}
			cp.Index(i).Set(ev)
		}
		return cp, nil
	case reflect.Struct:
		// No memo: structs are values. Fail-closed on unexported fields:
		// reflection cannot Set them, and skipping them would silently
		// LOSE data — both are contract violations. (Real integration:
		// the compiler generates clone code inside the defining package.)
		for i := 0; i < v.NumField(); i++ {
			if v.Type().Field(i).PkgPath != "" {
				return reflect.Value{}, fmt.Errorf(
					"iso: cannot clone %s — struct has unexported field %q (fail-closed: wrap in managed or define clone in-package)",
					v.Type(), v.Type().Field(i).Name)
			}
		}
		cp := reflect.New(v.Type()).Elem()
		for i := 0; i < v.NumField(); i++ {
			fv, err := cloneValue(v.Field(i), memo)
			if err != nil {
				return reflect.Value{}, err
			}
			cp.Field(i).Set(fv)
		}
		return cp, nil
	case reflect.Interface:
		if v.IsNil() {
			return v, nil
		}
		return cloneValue(v.Elem(), memo)
	case reflect.Func:
		return reflect.Value{}, fmt.Errorf(
			"iso: cannot clone func value at a message boundary (func values cannot be isolated; wrap in managed or reject)")
	case reflect.Uintptr, reflect.UnsafePointer:
		return reflect.Value{}, fmt.Errorf(
			"iso: cannot clone %s at a message boundary (a raw address would share memory)", v.Kind())
	}
	if scalarKinds[v.Kind()] {
		return v, nil
	}
	return reflect.Value{}, fmt.Errorf("iso: unsupported kind %s at boundary", v.Kind())
}

// cloneInto fills the already-allocated pointee copy dst from src.
// The pointer-level memo entry (registered by the Pointer case) is what
// closes cycles: a field pointing back resolves to the copy itself.
func cloneInto(dst, src reflect.Value, memo map[memoKey]reflect.Value) error {
	for i := 0; i < src.NumField(); i++ {
		if src.Type().Field(i).PkgPath != "" {
			return fmt.Errorf(
				"iso: cannot clone %s — struct has unexported field %q (fail-closed: wrap in managed or define clone in-package)",
				src.Type(), src.Type().Field(i).Name)
		}
		fv, err := cloneValue(src.Field(i), memo)
		if err != nil {
			return err
		}
		dst.Field(i).Set(fv)
	}
	return nil
}
