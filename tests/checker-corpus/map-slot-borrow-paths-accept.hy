// ============================================================================
// map-slot-borrow-paths-accept — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: the LEGAL spellings of a map-slot borrow, kept accepted so the new
// HYB-E3104 call-argument rule cannot widen into a rule that refuses buildable
// programs. Four paths in one program:
//
//   [P1] `n := &e.services["a"]`   — the O8 slot-loan initializer (borrow is
//                                    the whole value of the binding).
//   [P2] `m := (&e.services["b"])` — the same form through parentheses; parens
//                                    are position-transparent for a loan.
//   [P3] the loan's last-use store-back — `n.replicas = 3` / `m.replicas = 4`
//                                    are what the emitter turns into
//                                    copy-out / store-back.
//   [P4] `tmp := e.services["a"]` then `touch(&tmp)` — the corrected
//                                    call-argument pattern: copy the element
//                                    out of the map and pass the address of
//                                    the copy (a local struct IS addressable),
//                                    so `&tmp` needs no loan lowering.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) and O3's slot-loan form;
// the paired REJECT fixture is `map-slot-borrow-callarg-reject.hy`.
//
// ---- Why this is an ACCEPT ----
// Each path was checked, built and RUN on this branch before the fixture was
// written: `hyb check` reports 0 diagnostics, `go build` succeeds and the
// binary exits 0. The printed line pins the store-back semantics: `bump`
// writes through the loans (slot "a" = 3, slot "b" = 4) while `touch(&tmp)`
// mutates only the copy (tmp.replicas = 7) and leaves the map slot at 3 —
// which is exactly why the copy route is the buildable rewrite of
// `touch(&e.services["a"])`.
//
// ---- Expected verdict (reproduced as a real build + run first) ----
// EXPECTED-VERDICT: ACCEPT
// EXPECTED-OUTPUT: 3 4 7
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct service {
    replicas int
    tag      string
}

struct env {
    services map[string]service
}

fn touch(s *service) {
    s.replicas = 7
}

fn bump(e *env) {
    n := &e.services["a"]
    n.replicas = 3
    m := (&e.services["b"])
    m.replicas = 4
}

fn main() {
    e := env{services: map[string]service{"a": {replicas: 1, tag: "t"}, "b": {replicas: 2, tag: "u"}}}
    bump(e)
    tmp := e.services["a"]
    touch(&tmp)
    ra := e.services["a"].replicas
    rb := e.services["b"].replicas
    rc := tmp.replicas
    fmt.println("{ra} {rb} {rc}")
}