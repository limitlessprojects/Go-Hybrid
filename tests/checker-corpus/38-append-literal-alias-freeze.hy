// ============================================================================
// 38-append-literal-alias-freeze — Checker corpus #38 (added 2026-10-15).
//
// PURPOSE: the real-world face of the same root cause as 36 — `append` inside a
// composite literal. `append(base, 4)` with `cap(base) == 8` does NOT
// reallocate, so the result shares `base`'s backing array: `b.items[0]` and
// `base[0]` are the SAME element. The literal's field value is a call, and
// `Expr::root_ident()` returns None for a call, so no may-share/alias edge was
// recorded for `b` and the freeze never reached `base`:
//
//   hyb check  -> OK (0 diagnostics)          [pre-fix]
//   go run -race ->
//     WARNING: DATA RACE
//     Write at 0x... by goroutine 7:  main.worker()
//     Previous write at 0x... by main goroutine:  main.main()
//     Found 1 data race(s)   (exit status 66)
//
// This is why `append` matters: the `define` form (`t := append(base, 4)`) was
// already conservative — corpus 11-ish behaviour, and the generic/identity Call
// arm of #73 covers calls-with-ident-arguments — but wrapping the SAME append in
// a literal (or re-slicing it, or returning it from a helper) walked straight
// past the recorder. The fix is the same generalized collector as 32.
//
// UNCONTESTED: identical entailment to 32 — an append that does not reallocate
// hands back the SAME backing array, which is exactly the storage-identity
// argument O5.2 rests on. `append` in `define` position already rejects.
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `base` ([]int, len 3 cap 8) and `b.items` (the append
//   result — same backing array), `done`-free simple spawn, `b` (struct value
//   carrying the slice).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Sendability / check sites (task spawn / channel send) ----
//   [S1] `task worker(move b)` — OWNED MOVE of the struct. Legal as a
//        sendability question; the freeze starts here and must cover the
//        element storage `b.items` shares with `base`.
//   [S2] the write `base[0] = i` — VIOLATION (A1b): the sender writes an
//        element of the storage it just handed to the task.
//
// ---- Expected verdict (registered 2026-10-15) ----
// EXPECTED-VERDICT: REJECT A1b
//   REJECT — A1b alias/may-share form. Clean append variant of 36.
// RUNTIME-HAZARD (pre-fix shape only): accepted with 0 diagnostics before the
//   generalized collector; `go run -race` reports one write/write DATA RACE on
//   the shared element. After the fix the program must NOT build.
// ============================================================================

struct box {
    items []int
}

fn worker(b box) {
    for i in 0..200000 {
        b.items[0] = i
    }
}

fn main() {
    base := make([]int, 3, 8)         // capacity 8 > len 3: append stays put
    b := box{items: append(base, 4)}  // literal field holds an append result —
                                      // the SAME backing array as `base`
    task worker(move b)               // spawn site [S1] — owned move

    for i in 0..200000 {
        base[0] = i                   // VIOLATION (A1b): same element the task
    }                                 // writes through `b.items[0]`
}
