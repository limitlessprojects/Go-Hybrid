// ============================================================================
// 61-string-escape-fail-closed — Checker corpus #61.
//
// PURPOSE: malformed `\x` / `\u` / `\U` escape sequences in interpreted
// string literals used to be SILENTLY ACCEPTED with a corrupted value.
// `escape_char` returned `None` from its hex-digit error paths without
// recording the diagnostic both callers assume ("diag recorded"), so the
// string lexer kept one bumped char verbatim and continued: `"\u{41}"`
// lexed to the value "41}" (the `\u{` dropped, the rest leaked as literal
// text) and `hyb check` reported 0 diagnostics. The emitted Go printed
// `len=6` for a string the programmer wrote as one code point. That
// violates the fail-closed premise (GUARANTEES.md row 4's spirit: what the
// compiler cannot prove, it names — an accepted program must never carry a
// silently wrong value).
//
// Per LX7 interpreted strings use the GO escape set plus interpolation:
// `\u` takes exactly four hex digits (`\u00e9`), and the `\u{…}` brace
// form is LX6 rune-literal syntax only. A brace after `\u` in a string is
// therefore a malformed escape — HYB-E0002 (rule LX7), never a silent
// pass. `\xG1`, `\u12G4`, `\ud800` (surrogate) and `\U0011FFFF` (above
// U+10FFFF) are the same family (unit tests in hy-syntax/lexer.rs).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT LX7
// EXPECTED-DIAGNOSTIC: HYB-E0002
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    s := "\u{41}"
    fmt.println("s=[{s}] len={len(s)}")
}
