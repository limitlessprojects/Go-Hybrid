// ============================================================================
// prelude-local-shadow-controls-accept — Checker corpus (bounty track:
// "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT half of issue #4695 variant E: every legal neighbour
// the new local-shadow check must NOT reject. A careless version of the
// rule (e.g. "any local named hy_pop anywhere in the file") rejects all
// three of these, and all three build cleanly today.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a ACCEPT ----
//   1. A local `hy_pop` declared AFTER the `.pop()` call: Go scoping means
//      the lowered `hy_pop(&stack)` call resolves to the package-level
//      helper; the later binding shadows nothing before it.
//   2. A local `hy_pop` confined to a sibling `if` block: the binding is
//      popped with its block, so the `.pop()` call after the block still
//      resolves to the helper.
//   3. A local `hy_pop` with no `.pop()` use anywhere: there is no helper
//      call for the local to capture, so nothing can collide.
//
// ---- Expected verdict (each shape verified to build before committing) ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Paired fixture ----
// `prelude-local-shadow-reject.hy` holds the colliding shape, so these
// controls cannot be weakened into accepting the real collision.
//
// ---- Annotations / managed ----
// None.
// ============================================================================

fn after_call() {
    stack := []int{1, 2, 3}
    popped := stack.pop()
    hy_pop := 1
    fmt.println("{hy_pop} {popped}")
}

fn sibling_block() {
    stack := []int{1, 2, 3}
    if true {
        hy_pop := 1
        fmt.println("{hy_pop}")
    }
    popped := stack.pop()
    fmt.println("{popped}")
}

fn no_pop_use() {
    hy_pop := 1
    stack := []int{1, 2, 3}
    fmt.println("{hy_pop} {stack[0]}")
}

fn main() {
    after_call()
    sibling_block()
    no_pop_use()
}
