// ============================================================================
// method-return-alias-freeze — corpus GROWTH. COVERAGE GAP IT CLOSES: corpus
// 26/31 pin the alias edge for a free FUNCTION (`alias := same(agg)`), but the
// receiver-method form (`alias := h.map_ref()`) appeared in the corpus nowhere.
// The behaviour is asserted at unit level by
// `hy-check/tests/loop_backedge_shadow_and_range.rs::
// user_method_returning_map_records_may_share_with_receiver`; this fixture is
// the corpus-level pin of the same edge (the corpus is the spec layer, so a
// shape that only a Rust unit test guards is not pinned by the harness).
//
// PURPOSE: a method that returns the receiver's GC field returns a header that
// aliases the receiver's storage (the call-return alias edge, corpus 26/40).
// Method receivers do not create a fresh container: after the receiver struct
// is move-transferred, a write through the returned alias is use-after-move.
//
// ---- Sendability / check sites ----
//   [S1] `alias := h.map_ref()` — the returned map header aliases `h.m`.
//   [S2] `task worker(move h, done)` — owned move of the receiver struct; the
//        freeze reaches every live alias of its fields.
//   [S3] `alias["main"]++` — write through the live alias after the move.
//
// ---- Expected verdict (derived from the call-return alias edge + O5.2) ----
// EXPECTED-VERDICT: REJECT A1b
// ============================================================================

struct holder {
    m map[string]int
}

fn (h holder) map_ref() map[string]int {
    return h.m
}

fn worker(h holder, done chan<- bool) {
    h.m["task"]++
    done <- true
}

fn main() {
    h := holder{m: map[string]int{}}
    alias := h.map_ref()
    done := make(chan bool, 1)
    task worker(move h, done)
    alias["main"]++
    <-done
}
