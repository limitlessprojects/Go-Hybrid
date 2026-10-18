// ============================================================================
// 66-addr-of-call-read-control — Checker corpus #66 (issue #435)
//
// PURPOSE: the ACCEPT control for #65. The #435 fix walks address-of in
// `collect_ref_roots`, which must not turn a read-only call into a may-share
// edge: here `read(&h)` takes the address but returns a scalar, so nothing
// aliases `h` afterwards. A checker that rejects this is over-strict.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `read` observes `h` through the pointer and returns
//   an int. `n` is a scalar; `move h` transfers the struct and nothing else
//   can reach it.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task worker(move h)` — owned transfer of a value with no live alias.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    m map[string]int
}

fn read(h *holder) int {
    return len(h.m)
}

fn worker(h holder) {
    h.m["task"]++
}

fn main() {
    var h holder
    h.m = map[string]int{}

    n := read(&h)                   // address taken, scalar returned: no alias

    task worker(move h)             // [S1] safe: nothing else reaches `h`

    fmt.println("{n}")
}
