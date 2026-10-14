// 30-array-param-share-move-reject.hy — issue #75, ARRAY twin of corpus 29:
// moving a shallow-copied fixed-ARRAY peer of a parameter also escapes the
// O5.1 non-owned check.
//
// EXPECTED-VERDICT: REJECT A1b
//
// Background
// ----------
// Same root cause as 29: `check_non_owned_move` consulted the binding's
// own origin and its `aliases`-chain root only. A fixed array is not a
// reference alias (`record_alias` skips it), so `a2 := a` recorded a
// may-share edge (#27: Array(elem) recurses into the element shape, and
// `[2]map[string]int` carries a map) but NO origin. Before the fix
// `hyb check` reported OK (0 diagnostics) on the program below while the
// emitted Go raced (`fatal error: concurrent map writes` reproduced on
// `go run`). The map twin (`m2 := m`) was already rejected via the
// alias+origin-inheritance path; the struct twin is corpus 29.
//
// Rationale (hand-derived)
// ------------------------
// `a2 := a` is a SHALLOW array copy: both headers reference the SAME
// backing map elements. O5.1's rationale for rejecting `move a` ("the
// caller retains the argument") applies identically to `move a2`: the
// storage is not exclusively owned by the sending frame, and freezing
// `a2` cannot reach the caller's live `a` (O5.2 freeze-reach).
//
// Sites
// -----
// WRITE 1: aworker body `a[0]["k"] = i` (task side, through the moved peer)
// WRITE 2: main loop `a[0]["k"] = i` (caller side, through the local that
//          was passed in) — both mutate the SAME map.
// The spawn `task aworker(move a2, done)` is the rejection site (A1b).
// The accept side (pure-local peer) is pinned in
// hy-check/tests/param_share_move.rs.

fn aworker(a [2]map[string]int, done chan<- bool) {
    for i in 0..10000 {
        a[0]["k"] = i // task-side WRITE 1 through the shallow-copied header
    }
    done <- true
}

fn aouter(a [2]map[string]int, done chan<- bool) {
    a2 := a // shallow array copy: may-share peer of param a
    task aworker(move a2, done) // REJECTION SITE: moving the peer escapes O5.1
}

fn main() {
    var a [2]map[string]int
    a[0] = map[string]int{"k": 0}
    done := make(chan bool, 1)
    aouter(a, done)
    for i in 0..10000 {
        a[0]["k"] = i // caller-side WRITE 2: same storage aworker mutates
    }
    <-done
}
