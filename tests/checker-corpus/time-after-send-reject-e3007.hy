// ============================================================================
// time-after-send-reject-e3007 — Checker corpus (2026-10-24): a send on a
// `time.after` deadline channel must reject with HYB-E3007.
//
// PURPOSE: `package_result("time", "after")` modeled the deadline channel as
// a DIRECTIONLESS `Chan(Unknown)`, so `deadline <- v` passed `hyb check` with
// 0 diagnostics — while Go's `time.After` returns `<-chan time.Time` and the
// emitted `deadline <- v` fails to compile (`invalid operation: cannot send
// to receive-only channel`). An accepted program emitted unbuildable Go —
// the same accepted-should-emit-buildable-Go family as the send-target
// classification (#77) and the iterable check (#411). Found by adversarial
// probing of the foreign-descriptor stubs (foreign.rs TECH DEBT header):
// the shape layer carried no channel direction for foreign results.
//
// ---- Tier-0 rule set applied to each value ----
// `deadline` (chan, receive-only by construction — `time.after` produces a
//   `<-chan` value; the channel REFERENCE is class 3 safe-by-type, but the
//   send PROTOCOL on it is invalid Go); `v` (int — immutable primitive).
// The defect is not a value-share across tasks: it is a buildable-Go
// contract violation on the send statement itself.
// ---- Expected verdict: REJECT (HYB-E3007, the send-target classification;
// the receive side `v := <-deadline` stays ACCEPT and is covered by example
// 04's deadline pattern). ----

// EXPECTED-VERDICT: REJECT A1c
// (the send-target classification — #77; the pinned HYB code is E3007)

fn main() {
    deadline := time.after(time.seconds(5))
    deadline <- 1
}
