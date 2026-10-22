// ============================================================================
// multivalue-bind-source-count-reject — Checker corpus (issue #929).
//
// PURPOSE: multi-value binding sources whose yield cannot satisfy the
// bound name count. A channel receive and a map index yield at most two
// values (`v` or `v, ok`); indexing a slice, array, or string yields
// exactly one (no comma-ok form); a select receive arm with 3+ names
// crashes the emitter's select lowering; a 1-result call bound to 2+
// names is a plain assignment mismatch.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — T23 (HYB-E2002) for
// the multi-value-capable sources; HYB-E1002 (assignment mismatch) for
// the 1-result call shape. Go refuses every shape below with
// `assignment mismatch: N variables but ... values`.
//
// ---- Why this is a REJECT ----
// `check_multi_value_bind` only consulted `static_result_count` (calls)
// and returned early on 1-result sources; non-call sources (receive,
// index) bypassed arity checking entirely, and select receive arms never
// checked their name count. The fix adds the source-count bound
// (`static_bind_source_bound`) next to the call path in tuple.rs and the
// select-recv name guard, fail-closed before emission. Valid 1-value and
// comma-ok 2-value forms stay legal (accept controls pinned).
//
// Sibling take: harvested from issue #929 (magomedta) whose companion PR
// #930 rotted bot:stale-base — author credit magomedta (the #856/#960
// precedent).
//
// machine-readable verdict marker below; regression pins live in
// `hy-check/tests/binding_shape_guards.rs`.
// ============================================================================

fn main() {
    ch := make(chan int, 1)
    a, b, c := <-ch
    _ = a
    _ = b
    _ = c

    m := map[string]int{"k": 1}
    d, e, f := m["k"]
    _ = d
    _ = e
    _ = f

    xs := []int{1, 2}
    g, h := xs[0]
    _ = g
    _ = h
}

// EXPECTED-VERDICT: REJECT BUILDABLE
