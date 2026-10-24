// ============================================================================
// receiver-type-declared-controls-accept — Checker corpus (issue #1575).
//
// PURPOSE: every legal method receiver must keep working after the #1575
// pass-2 walk. The walk only rejects a receiver base that names NOTHING
// declared in the file (or a foreign dotted type).
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — accept side.
//
// ---- Why this is an ACCEPT ----
// Go allows a method whose receiver type is declared in the same file, in
// ANY source order (`later` is declared after its method). A zero-field
// struct is a defined type even though the shape table skips it (#722). A
// generic struct receiver and an enum receiver are defined types too.
//
// (A reject fixture is not expressible here: HYB-E2007 has no
// `tier0_rule_id` in hy-diag/data/diagnostics.json, so the corpus harness
// cannot pin the rule id a REJECT row requires. The reject half lives in
// compiler/crates/hy-check/tests/receiver_type_declared_1575.rs.)
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

// forward reference: the receiver type is declared AFTER the method
fn (x *later) m() int {
    return x.a
}

struct later {
    a int
}

struct empty {}

fn (x *empty) empty_m() int {
    return 1
}

struct box[t] {
    v t
}

fn (b *box[t]) get() t {
    return b.v
}

enum color {
    red,
    blue
}

fn (c *color) name() int {
    return 1
}

fn main() {
    var v later
    fmt.println(v.m())

    var e empty
    fmt.println(e.empty_m())

    var b box[int]
    b.v = 1
    fmt.println(b.get())

    var c color
    fmt.println(c.name())
}
