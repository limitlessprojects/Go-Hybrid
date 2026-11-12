package regexbinding

import (
	"errors"
	"runtime"
	"unicode/utf8"
	"unsafe"

	"hybrid.program/hybridge"
)

// The binding's error surface (Spike 5 sentinel shape).
var (
	ErrRegex = errors.New("regex: pattern or text invalid")
	ErrABI   = errors.New("regexbinding: hyrt ABI handshake mismatch (rebuild the runtime)")
)

// crossString: the zero-copy view of a string argument for one crossing.
// Returns the string's own backing pointer + len — no []byte copy, no
// runtime.Pinner (spike7-zerocopy-string). Same soundness conditions as
// jsonbinding.crossString: strings are immutable, the callee contract is
// the SPEC §9 O3 view loan (read-only, never retained), the caller keeps
// the string alive with runtime.KeepAlive until the crossing returns,
// and Go's heap is non-moving. Empty string crosses as (0, 0).
func crossString(s string) (uintptr, uintptr) {
	if len(s) == 0 {
		return 0, 0
	}
	return uintptr(unsafe.Pointer(unsafe.StringData(s))), uintptr(len(s))
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
	if !utf8.ValidString(pattern) || !utf8.ValidString(text) {
		return false, ErrRegex
	}
	pptr, plen := crossString(pattern)
	tptr, tlen := crossString(text)
	v := int64(hybridge.CrossDeep4(FnIsMatch, pptr, plen, tptr, tlen))
	runtime.KeepAlive(pattern)
	runtime.KeepAlive(text)
	switch v {
	case 0:
		return false, nil
	case 1:
		return true, nil
	default:
		return false, ErrRegex
	}
}

// IsValid reports whether the pattern compiles (validity as a bool — no
// error channel; example 09's simple_pattern surface).
//
// Stack class: DEEP (CrossDeep2) — same Regex::new compilation and AST-drop
// stack profile as IsMatch.
func IsValid(pattern string) bool {
	if !utf8.ValidString(pattern) {
		return false
	}
	pptr, plen := crossString(pattern)
	v := hybridge.CrossDeep2(FnIsValid, pptr, plen)
	runtime.KeepAlive(pattern)
	return v == 1
}
