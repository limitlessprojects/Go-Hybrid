// ============================================================================
// loop-fresh-copy-move-reject -- issue #372
//
// PURPOSE: pin the loop back-edge fresh-copy bypass. `chunk := chunk_of(words)`
// is a fresh copy per call summary, but the alias/may-share recorder still
// linked `chunk -> words`, and check_loop_backedge's fresh-copy collection
// resolved the alias chain instead of rooting at the moved binding -- so the
// back-edge check never saw `words` as moved inside the loop. The second
// iteration then moved the same slice into a second live task: two workers
// mutate one backing array (`WARNING: DATA RACE` under `go run -race`).
// Three companion PRs (#373/#421/#702) closed before a landing; the repro
// rejects on current main -- this file pins it.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `words` is a mutable slice; iteration 1 moves it to a
//   live worker; iteration 2's `task worker(move words, done)` moves it again
//   while the first task still writes `shared[0]` -- use-after-move + shared
//   backing array (O5.2). The `len(chunk) == 0` read of `words` through the
//   fresh copy does not break the edge.
// Layer 2/3: none.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT -- A1b/E3003 (use-after-move on the loop back-edge).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn chunk_of(src []int) []int {
    out := []int{}
    out = append(out, src[0])
    return out
}

fn worker(shared []int, done chan int) {
    shared[0] = shared[0] + 1
    done <- shared[0]
}

fn main() {
    done := make(chan int, 4)
    words := []int{1, 2, 3}
    for i in 0..2 {
        chunk := chunk_of(words)
        if len(chunk) == 0 {
            return
        }
        task worker(move words, done)
    }
    <-done
    <-done
}
