// ============================================================================
// a5-escape-via-user-fn-param-controls-accept — Checker corpus (A5 /
// HYB-E3013). Controls for `a5-escape-via-user-fn-param-reject.hy`:
//   - a user fn that calls its fn parameter synchronously (no escape)
//   - a wrapper whose foreign call is inside `managed("...", "interop")`
//     (the escape was audited where it happens)
//   - a read-only closure passed to an escaping wrapper (no captured write)
//
// RULE CITATION: SPEC-LANGUAGE §8.5 C9; GUARANTEES.md Row 9.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

import "time"

fn now(f fn()) {
    f()
}

fn audited(f fn()) {
    managed("the callback only touches state the caller no longer uses", "interop") {
        _ = time.after_func(0, f)
    }
}

fn later(f fn()) {
    _ = time.after_func(0, f)
}

fn main() {
    m := map[string]int{"a": 1}
    now(fn() {
        m["now"] = 1
    })
    other := map[string]int{}
    audited(fn() {
        other["audited"] = 1
    })
    later(fn() {
        _ = len(m)
    })
    time.sleep(time.milliseconds(20))
    fmt.println("{len(m)}")
}
