package regexbinding

import (
	"errors"
	"runtime"
	"unsafe"

	"hybrid.program/hybridge"
)

// The binding's error surface (Spike 5 sentinel shape).
var ErrRegex = errors.New("regex: pattern or text invalid")

// pinString: the copy-small pinned borrow for one string argument using a
// caller-owned stack runtime.Pinner (avoiding Pinner heap escape and empty-string
// slice allocation).
func pinString(s string, p *runtime.Pinner) (uintptr, uintptr) {
	if len(s) == 0 {
		return 0, 0
	}
	b := []byte(s)
	p.Pin(&b[0])
	return uintptr(unsafe.Pointer(&b[0])), uintptr(len(b))
}

func decodeMatchStatus(v int64) (bool, error) {
	if v != 0 && v != 1 {
		return false, ErrRegex
	}
	return v == 1, nil
}

// IsMatch reports whether text matches pattern (compile per call — the
// example surface calls it once; hot loops pin the compiled form post-MVP).
//
// Stack class: DEEP (CrossDeep4). Regex::new compiles the pattern on every
// call: regex_automata::meta::strategy::new allocates a 7,648 B frame (and
// up to 22,976 B across the acyclic compile chain), and dropping the parsed
// regex_syntax::ast::Ast / ClassSet recurses on bracket nesting depth. That
// exceeds the 4,096 B DEFAULT fast-path guard even for a 1-byte pattern.
func IsMatch(pattern, text string) (bool, error) {
	var p runtime.Pinner
	pptr, plen := pinString(pattern, &p)
	tptr, tlen := pinString(text, &p)
	v := int64(hybridge.CrossDeep4(FnIsMatch, pptr, plen, tptr, tlen))
	p.Unpin()
	runtime.KeepAlive(&p)
	return decodeMatchStatus(v)
}

// IsValid reports whether the pattern compiles (validity as a bool — no
// error channel; example 09's simple_pattern surface).
//
// Stack class: DEEP (CrossDeep2) — same Regex::new compilation and AST-drop
// stack profile as IsMatch.
func IsValid(pattern string) bool {
	var p runtime.Pinner
	pptr, plen := pinString(pattern, &p)
	v := hybridge.CrossDeep2(FnIsValid, pptr, plen)
	p.Unpin()
	runtime.KeepAlive(&p)
	return v == 1
}
