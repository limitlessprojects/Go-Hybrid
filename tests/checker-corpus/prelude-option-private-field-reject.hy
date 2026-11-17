// ============================================================================
// prelude-option-private-field-reject — Checker corpus (bounty track: "grow the adversarial corpus", issue #4532).
//
// PURPOSE: pins that prelude-private fields of `option[T]` (`val`, `ok`) are not
//   readable or writable from Hybrid source. A `none` payload cannot be silently
//   read, and a `some` cannot be manufactured without a constructor.
//
// RULE CITATION: SPEC-LANGUAGE T20 + HYB-E1002 (member undefined on Hybrid surface).
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `o` (option[int]).
//   Layer 2 (Rust-side owned): none.
//
// ---- Sendability / check sites ----
//   [S1] `o.val = 42`: field selection on option[T] must reject undefined field.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT HYB-E1002
//   REJECT — HYB-E1002. `option[int]` exposes `.unwrap()`, `.unwrap_or(...)`, `.is_some()`,
//   `.is_none()`, not the private Go representation fields `val` and `ok`.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    var o option[int] = none
    o.val = 42
    fmt.println("o.val: {o.val}")
}
