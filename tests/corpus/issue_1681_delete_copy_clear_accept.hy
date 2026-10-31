// ============================================================================
// issue_1681_delete_copy_clear_accept — Corpus fixture for issue #1681.
//
// PURPOSE: verify valid `len` on map/slice/string literals, `cap` on slice
// literals, and `delete` on map variables/literals remain accepted with
// zero diagnostics.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

fn main() {
    m := map[string]int{"a": 1, "b": 2}
    delete(m, "a")
    delete(map[string]int{"x": 10}, "x")
    n1 := len(map[string]int{"k": 7})
    n2 := len([]int{1, 2, 3})
    c1 := cap([]int{1, 2, 3})
    _ = n1 + n2 + c1
}
