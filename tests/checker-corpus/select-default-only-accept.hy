// ============================================================================
// select-default-only-accept — Checker corpus: a select with ONLY a default
// arm and no comm clauses is valid Go (non-blocking poll) and must be
// accepted by the checker.
// ---- Expected verdict: ACCEPT. ----
// EXPECTED-VERDICT: ACCEPT

fn main() {
    select {
        default: fmt.println("nothing ready")
    }
}
