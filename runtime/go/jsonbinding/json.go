package jsonbinding

import (
	"encoding/json"
	"errors"
	"runtime"
	"unsafe"

	"hybrid.program/hybridge"
)

// The binding's error surface (Spike 5 sentinel shape; wrap-on-demand, E5).
var (
	ErrJSON = errors.New("serde_json: operation failed")
	ErrABI  = errors.New("serde_json: hyrt ABI handshake mismatch")
)

// Crossing CLASS ledger (row 4: per-callee stack budgets; the full rationale
// lives in interfaces/descriptors.md). The rule: a crossing may take the
// guarded fast path ONLY if the callee's worst-case stack use is an UPPER
// BOUND below the guard — the guard proves headroom at ENTRY and nothing
// after. serde_json RECURSES on value nesting (parse: remaining_depth=128;
// clone/drop/serialize walk the tree), so every fn that can touch a nested
// value is DEEP-class regardless of input shape; O(1) table/leaf ops are
// SMALL-class (<= 1 KiB argued bound, guard 1024).
//
//   DEEP  (CrossDeep* — pooled 1 MiB guarded stack): Parse, GetArray (clone),
//         ArrayGet (clone), Set (drop of the REPLACED value), Release (drop),
//         ToStringPretty/valueText (serialize) — plus the cleanup goroutine's
//         release, which runs on an arbitrary goroutine.
//   SMALL (Cross*S — guard 1024): NewObject, NewArray, GetInt, GetString
//         (string clone is a heap copy, not a tree walk), ArrayLen,
//         PushString — all O(1), no recursion on the value tree.
//
// SOUNDNESS NOTE (found while classifying, fixed by this table): routing
// Parse through the default 4096-guard fast path was UNSOUND for deep
// inputs on shallow stacks — the guard proves 4096 B of headroom, but a
// 128-deep parse can need an order of magnitude more. The stress test
// (TestDeepNestingOnConstrainedStack) pins the fix.

// Value is a Rust-owned JSON value handle. Zero id = released/moved.
type Value struct{ h *handle }

type handle struct {
	id uint64
	cl runtime.Cleanup
}

func wrap(id uint64) Value {
	h := &handle{id: id}
	// DEEP class: drop recurses on the value tree; cleanup goroutines run
	// on arbitrary stacks. Keep the Cleanup token so setHandle/releaseNow
	// can Stop() it before a moved or explicitly released slot is recycled.
	h.cl = runtime.AddCleanup(h, func(id uint64) {
		if id != 0 {
			hybridge.CrossDeep1(FnRelease, uintptr(id))
		}
	}, id)
	return Value{h: h}
}

func (v Value) releaseNow() {
	if v.h == nil || v.h.id == 0 {
		return
	}
	id := v.h.id
	v.h.cl.Stop()
	v.h.id = 0
	hybridge.CrossDeep1(FnRelease, uintptr(id))
}

// pinCrossKey: the copy-small pinned borrow for a key/string argument.
// Returns the byte copy's pointer + len and a release func.
func pinCrossKey(s string) (uintptr, uintptr, *runtime.Pinner, []byte) {
	b := []byte(s)
	var p runtime.Pinner
	if len(b) > 0 {
		p.Pin(&b[0])
		return uintptr(unsafe.Pointer(&b[0])), uintptr(len(b)), &p, b
	}
	// Empty key: pass (0, 0) — hyrt's HyStr::as_str maps len==0 to "".
	return 0, 0, &p, b
}

// Parse parses JSON text into a fresh value handle.
func Parse(data []byte) (Value, error) {
	var id uint64
	var p runtime.Pinner
	var dptr uintptr
	if len(data) > 0 {
		p.Pin(&data[0])
		dptr = uintptr(unsafe.Pointer(&data[0]))
	}
	p.Pin(&id)
	// DEEP class: from_str recurses up to remaining_depth=128 (see ledger).
	st := hybridge.CrossDeep3(FnParse, dptr, uintptr(len(data)), uintptr(unsafe.Pointer(&id)))
	p.Unpin()
	runtime.KeepAlive(&p)
	runtime.KeepAlive(data)
	if st != statusOK {
		return Value{}, ErrJSON
	}
	return wrap(id), nil
}

// NewObject / NewArray: fresh containers.
func NewObject() Value {
	var id uint64
	hybridge.Cross1S(FnNewObject, uintptr(unsafe.Pointer(&id))) // SMALL: table insert, O(1)
	return wrap(id)
}

func NewArray() Value {
	var id uint64
	hybridge.Cross1S(FnNewArray, uintptr(unsafe.Pointer(&id))) // SMALL: table insert, O(1)
	return wrap(id)
}

// Array returns the array at v[key], MATERIALIZED as a Go slice of handles
// (len + N element crossings inside the binding) — the .hy surface iterates
// `for u in v.array(k)?` as a plain slice range.
func (v Value) Array(key string) ([]Value, error) {
	if v.h == nil || v.h.id == 0 {
		return nil, ErrJSON
	}
	var arrID uint64
	kptr, klen, pin, kb := pinCrossKey(key)
	pin.Pin(&arrID)
	// DEEP class: as_array().clone() recurses on the array's contents.
	st := hybridge.CrossDeep4(FnGetArray, uintptr(v.h.id), kptr, klen, uintptr(unsafe.Pointer(&arrID)))
	pin.Unpin()
	runtime.KeepAlive(pin)
	runtime.KeepAlive(kb)
	runtime.KeepAlive(v.h)
	if st != statusOK {
		return nil, ErrJSON
	}
	var n uint64
	var lenPin runtime.Pinner
	lenPin.Pin(&n)
	lenSt := hybridge.Cross2S(FnArrayLen, uintptr(arrID), uintptr(unsafe.Pointer(&n))) // SMALL: len read
	lenPin.Unpin()
	runtime.KeepAlive(&lenPin)
	if lenSt != statusOK {
		hybridge.CrossDeep1(FnRelease, uintptr(arrID))
		return nil, ErrJSON
	}
	out := make([]Value, n)
	for i := range out {
		var eid uint64
		var elemPin runtime.Pinner
		elemPin.Pin(&eid)
		// DEEP class: elem.clone() recurses on the element's nesting.
		elemSt := hybridge.CrossDeep3(FnArrayGet, uintptr(arrID), uintptr(i), uintptr(unsafe.Pointer(&eid)))
		elemPin.Unpin()
		runtime.KeepAlive(&elemPin)
		if elemSt != statusOK || eid == 0 {
			for j := 0; j < i; j++ {
				out[j].releaseNow()
			}
			hybridge.CrossDeep1(FnRelease, uintptr(arrID))
			return nil, ErrJSON
		}
		out[i] = wrap(eid)
	}
	// The temporary array handle dies with the materialized slice.
	// DEEP class: drop recurses on the cloned tree.
	hybridge.CrossDeep1(FnRelease, uintptr(arrID))
	return out, nil
}

// Int returns the i64 at v[key].
func (v Value) Int(key string) (int64, error) {
	if v.h == nil || v.h.id == 0 {
		return 0, ErrJSON
	}
	var n int64
	kptr, klen, pin, kb := pinCrossKey(key)
	pin.Pin(&n)
	st := hybridge.Cross4S(FnGetInt, uintptr(v.h.id), kptr, klen, uintptr(unsafe.Pointer(&n))) // SMALL: scalar read
	pin.Unpin()
	runtime.KeepAlive(pin)
	runtime.KeepAlive(kb)
	runtime.KeepAlive(v.h)
	if st != statusOK {
		return 0, ErrJSON
	}
	return n, nil
}

// String returns the string at v[key] (one extra release crossing for the
// temporary string handle; the text itself copies out through the handle).
func (v Value) String(key string) (string, error) {
	if v.h == nil || v.h.id == 0 {
		return "", ErrJSON
	}
	var sid uint64
	kptr, klen, pin, kb := pinCrossKey(key)
	pin.Pin(&sid)
	// SMALL: string clone is a heap copy, not a tree walk.
	st := hybridge.Cross4S(FnGetString, uintptr(v.h.id), kptr, klen, uintptr(unsafe.Pointer(&sid)))
	pin.Unpin()
	runtime.KeepAlive(pin)
	runtime.KeepAlive(kb)
	runtime.KeepAlive(v.h)
	if st != statusOK {
		return "", ErrJSON
	}
	s := valueText(sid)
	hybridge.CrossDeep1(FnRelease, uintptr(sid)) // DEEP: drop recurses on the tree
	return s, nil
}

// PushString appends s to an array value (example 05's arr.push_string).
func (v Value) PushString(s string) error {
	if v.h == nil || v.h.id == 0 {
		return ErrJSON
	}
	sptr, slen, pin, sb := pinCrossKey(s)
	st := hybridge.Cross3S(FnPushString, uintptr(v.h.id), sptr, slen) // SMALL: Vec push + heap copy
	pin.Unpin()
	runtime.KeepAlive(pin)
	runtime.KeepAlive(sb)
	runtime.KeepAlive(v.h)
	if st != statusOK {
		return ErrJSON
	}
	return nil
}

// Set moves val into v[key]. The scalar surface (differential corpus 12/13)
// takes any Go scalar: a Value handle moves directly; bool/int/float/string
// box through a JSON round-trip (Marshal produces the exact JSON text, Parse
// allocates the Rust-side handle, set moves it in — the temp's cleanup is
// explicitly stopped when moved, or released immediately if setHandle fails).
func (v Value) Set(key string, val any) error {
	if hv, ok := val.(Value); ok {
		return v.setHandle(key, hv)
	}
	b, err := json.Marshal(val)
	if err != nil {
		return err
	}
	bv, err := Parse(b)
	if err != nil {
		return err
	}
	if err := v.setHandle(key, bv); err != nil {
		bv.releaseNow()
		return err
	}
	return nil
}

func (v Value) setHandle(key string, val Value) error {
	if v.h == nil || v.h.id == 0 || val.h == nil || val.h.id == 0 {
		return ErrJSON
	}
	kptr, klen, pin, kb := pinCrossKey(key)
	// DEEP class: insert DROPS the replaced value (if the key existed) and
	// drop recurses on the value tree.
	st := hybridge.CrossDeep4(FnSet, uintptr(v.h.id), kptr, klen, uintptr(val.h.id))
	pin.Unpin()
	runtime.KeepAlive(pin)
	runtime.KeepAlive(kb)
	runtime.KeepAlive(v.h)
	runtime.KeepAlive(val.h)
	if st != statusOK {
		val.releaseNow()
		return ErrJSON
	}
	val.h.cl.Stop()
	val.h.id = 0 // moved: the handle's payload now lives inside v
	return nil
}

// ToStringPretty serializes v (two-leg buffer protocol: the first crossing
// reports the needed length via status-3; the second copies).
func ToStringPretty(v Value) (string, error) {
	if v.h == nil || v.h.id == 0 {
		return "", ErrJSON
	}
	var need uint64
	var needPin runtime.Pinner
	needPin.Pin(&need)
	// DEEP class: serialize walks the value tree (see ledger).
	st := hybridge.CrossDeep4(FnToStringPretty, uintptr(v.h.id), 0, 0, uintptr(unsafe.Pointer(&need)))
	needPin.Unpin()
	runtime.KeepAlive(&needPin)
	if st == statusTryAgain && need > 0 {
		buf := make([]byte, need)
		var p runtime.Pinner
		p.Pin(&buf[0])
		p.Pin(&need)
		st = hybridge.CrossDeep4(FnToStringPretty, uintptr(v.h.id),
			uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)), uintptr(unsafe.Pointer(&need)))
		p.Unpin()
		runtime.KeepAlive(&p)
		runtime.KeepAlive(buf)
		runtime.KeepAlive(v.h)
		if st == statusOK {
			return string(buf[:need]), nil
		}
	}
	runtime.KeepAlive(v.h)
	if st != statusOK {
		return "", ErrJSON
	}
	return "", ErrJSON
}

// valueText reads a string-value's text (compact to_string of a Json::String
// is the quoted form; hyrt's get_string already stores the raw text as a
// Json::String, so to_string would QUOTE it — hy_json_value_text on a bare
// string value returns the quoted JSON. The binding therefore reads via the
// compact form and strips the quotes it added — one documented copy).
func valueText(id uint64) string {
	var need uint64
	var needPin runtime.Pinner
	needPin.Pin(&need)
	st := hybridge.CrossDeep4(FnToStringPretty, uintptr(id), 0, 0, uintptr(unsafe.Pointer(&need))) // DEEP: serialize
	needPin.Unpin()
	runtime.KeepAlive(&needPin)
	if st != statusTryAgain || need < 2 {
		return ""
	}
	buf := make([]byte, need)
	var p runtime.Pinner
	p.Pin(&buf[0])
	p.Pin(&need)
	hybridge.CrossDeep4(FnToStringPretty, uintptr(id), uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)), uintptr(unsafe.Pointer(&need)))
	p.Unpin()
	runtime.KeepAlive(&p)
	runtime.KeepAlive(buf)
	// Json::String's to_string is a quoted JSON string with escaped characters.
	var s string
	if err := json.Unmarshal(buf[:need], &s); err == nil {
		return s
	}
	// Fallback: strip the 2-quote wrapper if unmarshal fails.
	raw := string(buf[:need])
	if len(raw) >= 2 && raw[0] == '"' && raw[len(raw)-1] == '"' {
		return raw[1 : len(raw)-1]
	}
	return raw
}
