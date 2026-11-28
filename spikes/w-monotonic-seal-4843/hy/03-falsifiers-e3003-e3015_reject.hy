// ============================================================================
// 03-falsifiers-e3003-e3015_reject.hy — Wildcard M1/M2 fixture
//
// Pins Declared Falsifiers F2 (Remedy R2) and F3 (Remedy R3):
//   1. F2 / R2 (`HYB-E3003`): extracting `sub := buf[:1]` before `seal move buf`
//      places `sub` in `storage_component(buf)` (`aliases U may_share`); writing
//      `sub[0] = 99` after `seal move buf` rejects as HYB-E3003.
//   2. F3 / R3 (`HYB-E3015`): `seal move cfg` where `cfg` carries an interior
//      closure (`fn() int`), channel, or interface rejects as HYB-E3015
//      (`is_sealable(T)` representation-purity gate).
//
// EXPECTED-VERDICT: REJECT HYB-E3003 HYB-E3015
// ============================================================================
package main

struct stateful_cfg {
    name    string
    next_id fn() int
}

fn falsifier_f2_interior_subslice() {
    buf := []int{10, 20, 30}
    sub := buf[:1]              // `sub` enters `may_share(buf)`
    let snap = seal move buf    // freezes `buf` AND `sub` against writes (R2)
    sub[0] = 99                 // reject: HYB-E3003 — write through pre-seal alias
    _ = snap.len()
}

fn falsifier_f3_stateful_closure_field() {
    n := 0
    cfg := stateful_cfg{
        name: "prod",
        next_id: fn() int {
            n++
            return n
        },
    }
    let bad = seal move cfg     // reject: HYB-E3015 — `stateful_cfg` holds closure `fn() int`
    _ = bad
}
