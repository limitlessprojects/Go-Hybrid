// 29-struct-param-share-move-reject.hy — issue #75: moving a MAY-SHARE PEER
// of a parameter escapes the O5.1 non-owned check.
//
// EXPECTED-VERDICT: REJECT A1b
//
// Background
// ----------
// `check_non_owned_move` used to derive `non_owned_origin` from the
// binding's own origin or its `aliases`-chain root only. The MAY-SHARE
// graph (#27/#35 — undirected, covering shallow struct/array copies,
// re-slices, projections, and composite captures) was not consulted, and
// `record_alias`'s define-time origin inheritance only fires for
// reference-alias shapes (Map/Slice/Pointer). So the map twin
// (`m2 := m` with map param `m`) was correctly rejected HYB-E3004 via the
// alias+inherited-origin path, but the struct form below got a may-share
// edge and NO origin: `hyb check` reported OK (0 diagnostics) while the
// emitted Go raced (`fatal error: concurrent map writes` reproduced on
// `go run`).
//
// Rationale (hand-derived, pre-registration discipline per VERDICTS.md)
// ---------------------------------------------------------------------
// `h2 := h` is a SHALLOW struct copy: `h2` and param `h` reference the
// same backing map through separate struct headers. The O5.1 rationale
// for rejecting `move h` ("the caller retains the argument across
// intra-task calls") applies identically to `move h2`: the storage is
// not exclusively owned by the sending frame, and freezing `h2` cannot
// reach the caller's live `h` binding (O5.2 freeze-reach). Before the
// fix this program passed with 0 diagnostics and the emitted Go
// (`h2 := h; go worker(h2, done)` while `main` loops `h.m["k"] = i`)
// hit `fatal error: concurrent map writes`. The fix scans the may-share
// component for a non-owned origin at move time (freeze.rs
// `check_non_owned_move`); the pure-local accept side and the map/array
// twins are pinned in hy-check/tests/param_share_move.rs.
//
// Sites
// -----
// WRITE 1: worker body `h.m["k"] = i` (task side, through the moved peer)
// WRITE 2: outer loop `h.m["k"] = i` (caller side, through the param) —
//          both mutate the SAME map; a real Go data race.
// The spawn `task worker(move h2, done)` is the rejection site (A1b:
// `move` requires an exclusively owned local binding).

struct holder {
    m map[string]int
}

fn worker(h holder, done chan<- bool) {
    for i in 0..10000 {
        h.m["k"] = i // task-side WRITE 1 through the shallow-copied header
    }
    done <- true
}

fn outer(h holder, done chan<- bool) {
    h2 := h // shallow struct copy: may-share peer of param h
    task worker(move h2, done) // REJECTION SITE: moving the peer escapes O5.1
}

fn main() {
    h := holder{m: map[string]int{"k": 0}}
    done := make(chan bool, 1)
    outer(h, done)
    for i in 0..10000 {
        h.m["k"] = i // caller-side WRITE 2: same storage worker mutates
    }
    <-done
}
