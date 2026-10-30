// ============================================================================
// const-ident-negative-shift-reject-e6005 — Checker corpus (2026-10-26):
// B4 (HYB-E6005) must resolve FILE-LEVEL CONST REFERENCES in shift counts.
//
// PURPOSE: check_negative_shift folded only literal expressions
// (`x << -1`), so `x << neg` with `const neg = -2` passed `hyb check`
// with 0 diagnostics while Go treats a named constant exactly like its
// value — `invalid operation: negative shift count neg (untyped int
// constant -2)` at COMPILE time. An accepted program emitted unbuildable
// Go (issue #201 family). The registry's own out-of-scope note ("a
// negative VARIABLE shift count is a run-time panic") covers locals, NOT
// file-level consts: `neg` is an untyped constant, in scope.
//
// ---- Expected verdict: REJECT BUILDABLE [HYB-E6005]. ----

// EXPECTED-VERDICT: REJECT BUILDABLE

const neg = -2

fn main() {
    x := 1
    fmt.println(x << neg)
}
