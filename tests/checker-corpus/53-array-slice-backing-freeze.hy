// ============================================================================
// 53-array-slice-backing-freeze — Checker corpus #53.
//
// PURPOSE: a slice of an addressable array shares the array's backing
// storage, including when the element type is a primitive. `s := a[:]`
// on `var a [4]int` used to record no may-share edge (`shape_may_share`
// recurses only into reference elements, so `[4]int` looked unshared).
// `task worker(move s, done)` then froze `s` only. `hyb check` reported
// 0 diagnostics while the emitted Go (`s := a[:]`, `go worker(s)`, then
// `a[0] = ...`) raced under `go run -race` (`WARNING: DATA RACE`,
// exit status 66).
//
// A value copy of the array (`b := a`) and a slice of element copies
// (`[]int{a[0], a[1]}`) do NOT share and stay accepted (unit tests).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1b
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn worker(s []int, done chan<- bool) {
    s[0] = 1
    done <- true
}

fn main() {
    var a [4]int
    s := a[:]
    done := make(chan bool, 1)
    task worker(move s, done)
    a[0] = 2
    <-done
}
