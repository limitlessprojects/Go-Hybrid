// ============================================================================
// 01-d1-corpus19-move-vs-seal_accept.hy — Wildcard M1/M2 fixture
//
// Resolves SPEC-LANGUAGE.md §13.2 Open Decision D1 (corpus #19) + kills W-005's
// 40,000x clone wall at 10k nodes without weakening §9.2:
//   - `results <- move agg` followed by `len(agg)` STAYS REJECT HYB-E3003
//     (because receiver `sink` mutates `m["fox"]++`).
//   - `let snap = seal move agg` consumes exclusive ownership of `agg`
//     (O5.7: Live[R] -> Sealed[R], freezing `aliases(agg) U may_share(agg)`
//     against writes) and yields `sealed[map[string]int]` (Class 4b: Copy,
//     0 B/op, 0 allocs/op, broadcastable across N tasks + readable by sender).
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================
package main

import (
    "fmt"
    "strings"
)

const corpus = "the quick brown fox jumps over the lazy dog the fox runs hybrid"

fn worker(id int, snap sealed[map[string]int], done chan<- int) {
    fox_count, _ := snap.get("fox")
    done <- id * 100 + fox_count
}

fn main() {
    agg := map[string]int{}
    for w in strings.split(corpus, " ") {
        agg[w]++
    }

    // O5.7 monotonic transition: `agg` is consumed; `snap` enters Class 4b.
    let snap = seal move agg

    done := make(chan int, 2)
    task worker(1, snap, done)
    task worker(2, snap, done)

    // Sender continues reading `snap.len()` concurrently with workers (D1 twin):
    n := snap.len()
    a := <-done
    b := <-done
    fmt.println("words={n} sum={a + b}")
}
