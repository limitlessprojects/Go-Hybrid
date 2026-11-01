// ============================================================================
// issue_1681_delete_copy_clear_reject — Corpus fixture for issue #1681.
//
// PURPOSE: verify `hyb check` rejects `cap(map[...]{})` map literals,
// `len(nil)` / `cap(nil)` untyped nil operands, and `delete` calls whose
// first argument is a non-map literal (`HYB-E2005`).
//
// EXPECTED-VERDICT: REJECT BUILDABLE
// ============================================================================

fn main() {
    _ = cap(map[string]int{"a": 1})
    _ = len(nil)
    _ = cap(nil)
    delete(42, "k")
    delete("not-a-map", "k")
    delete([]int{1, 2, 3}, 0)
    delete(nil, "k")
}
