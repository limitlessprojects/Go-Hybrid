// ============================================================================
// option-fresh-payload-at-spawn-accept — ACCEPT-side control for
// option-map-alias-at-spawn (corpus GROWTH, bounty track: adversarial corpus tests).
//
// WHY THIS EXISTS: the reject case pins that `some(shared)` embeds the shared
// map's header, so a later write through `shared` is use-after-move. On its own
// that pin cannot distinguish the intended alias edge from a blanket "an option
// around a map is never moveable" rule. This control holds every syntactic
// feature of the reject case fixed — `option[map[string]int]`, a `match` in the
// callee, `task worker(move o, done)` — and removes ONLY the aliasing: the
// payload map is allocated inside the option expression, so no other binding
// holds its storage.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

fn worker(o option[map[string]int], done chan<- bool) {
    match o {
        some(m): m["task"]++
        none:    fmt.println("none")
    }
    done <- true
}

fn main() {
    o := some(map[string]int{})
    done := make(chan bool, 1)
    task worker(move o, done)
    <-done
}
