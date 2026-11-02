// ============================================================================
// select-two-recv-define-arms-accept — Checker corpus regression for the
// parser fix (2026-10-24): the arm-body terminator
// `looks_like_select_arm_head` recognized send arms, bare recv arms and
// comma-ok recv-define arms, but NOT the single-name recv-define arm
// (`v := <-ch:`). Any select whose SECOND or later arm was a named
// recv-define therefore failed to parse ("expected expression, found
// `:`") — the body loop swallowed the arm head as a local declaration
// and choked on the trailing colon. Corpus 24 only paired one named arm
// with a bare arm, so the gap had zero coverage.
//
// ---- Tier-0 rule set applied ----
// `a`/`b` are channels (concurrency-safe BY TYPE — class 3 per §8.4 C5);
// received ints are copies (immutable); `total` is main-local and never
// crosses a boundary. No spawn/send violation exists: the only sends are
// int literals into buffered channels before any task exists.
// ---- Expected verdict: ACCEPT (and must PARSE — the pre-fix parser
// rejected this shape outright). ----
// EXPECTED-VERDICT: ACCEPT

fn main() {
    a := make(chan int, 1)
    b := make(chan int, 1)
    a <- 1
    b <- 2
    total := 0
    select {
        v := <-a:
            total += v
        w, ok := <-b:
            if ok { total += w }
    }
    fmt.println(total)
}
