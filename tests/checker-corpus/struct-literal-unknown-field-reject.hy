// ============================================================================
// struct-literal-unknown-field-reject — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: a struct literal naming a field the struct does not declare.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// `s` declares only `a`, so `s{nope: 1}` is `unknown field nope in struct
// literal of type s`. The checker previously accepted the file with 0
// diagnostics.
//
// Closed by the #682 U14 rule (HYB-E1002,
// `buildable.rs::check_struct_literal`). The same rule also rejects a duplicate
// field name (U15, pinned in
// `hy-check/tests/struct_and_conversion_shape_guards.rs`), which cannot be shown
// in one file together with this one.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `struct-and-conversion-controls-accept.hy` holds the legal neighbours, so
// these rules cannot be widened into something that refuses buildable programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct s {
    a int
}

fn main() {
    q := s{nope: 1}
    fmt.println("{q.a}")
}
