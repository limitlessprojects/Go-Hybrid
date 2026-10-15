// ============================================================================
// 44-may-share-bridge-rebind — Checker corpus #44 (soundness issue #92,
// reported against main 8fb9238; transitive may-share under bridge rebind).
//
// PURPOSE: pin O5.2 / A1b on a rebind of an INTERMEDIATE shallow copy.
// `h2 := h1; h3 := h2` records the 2-hop chain h1—h2—h3. Rebinding the
// bridge (`h2 = holder{m: map[string]int{"fresh": 0}}`) called
// `FnState::share_clear("h2")`, which deleted h2's incident edges WITHOUT
// connecting its former peers — severing the path between `h1` and `h3`
// even though `h1.m` and `h3.m` remain the exact same map header. A later
// `task worker(move h1, done)` then left `h3` unfrozen: before the fix
// `hyb check` reported OK (0 diagnostics) while `go run` crashed with
// `fatal error: concurrent map writes`. The fix closes the graph under
// vertex elimination: share_clear connects the former peers pairwise
// (clique closure), so the component survives the bridge's departure.
// The accept-side guard is hy-check's const_write/alias suites plus the
// rebind-isolated control: a bridge whose ONLY peer is the moved root
// creates no closure pair, so a genuine fresh rebind never over-freezes.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `h1`/`h2`/`h3` (holder holding map - env - GC).
// Layer 2: `move h1` is the sole ownership transfer; the worker owns h1.
// Layer 3 (A1b): `h2 := h1` and `h3 := h2` are shallow copies — all three
//   share one map header; after `h2` is rebound away, `h1` and `h3` still
//   share it, so the move of `h1` must freeze `h3`.
// Layer 4: no loans; pure may-share freeze.
// Layer 5 (A5): the worker body mutates `h1.m`; main's loop mutates the
//   same map through `h3.m`.
// Layer 6 (O5.1): all storage is main-owned.
// Layer 7 (A7): `done <- true` hands the bool; buffered cap 1.
// Layer 8 (FC): the verdict is a rule rejection, not an unproven-name FC.
//
// EXPECTED: HYB-E3003 (write through a live alias of a moved value,
//   tier0_rule A1b / O5.2) at `h3.m["k"] = i`.
//
// ============================================================================
// EXPECTED-VERDICT: REJECT A1b

struct holder {
    m map[string]int
}

fn worker(h holder, done chan<- bool) {
    for i in 0..10000 {
        h.m["k"] = i
    }
    done <- true
}

fn main() {
    h1 := holder{m: map[string]int{"k": 0}}
    h2 := h1
    h3 := h2
    h2 = holder{m: map[string]int{"fresh": 0}}
    done := make(chan bool, 1)
    task worker(move h1, done)
    for i in 0..10000 {
        h3.m["k"] = i
    }
    <-done
}