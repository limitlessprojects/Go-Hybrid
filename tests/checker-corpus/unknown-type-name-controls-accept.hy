// ============================================================================
// unknown-type-name-controls-accept — Checker corpus (issue #1583).
//
// PURPOSE: every type name the language legitimately allows must keep working
// after the #1583 check. The reject half is `unknown-type-name-reject.hy`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — accept side.
//
// ---- Why this is an ACCEPT ----
// The check only rejects a BARE name that resolves to nothing. It accepts:
//   - a declared struct, a type alias, and a zero-field struct;
//   - a generic struct's type parameter in nested positions ([]t, map, option),
//     a generic argument that is itself a type parameter (box[t]), and a
//     generic fn's type parameter;
//   - Go's predeclared types and the SPEC conversion spellings (int, uint8,
//     float64, bool, string, u8, f32);
//   - the prelude types `option` and `set`.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

struct point {
    x int
    y int
}

struct named_cb {
    // A parameter list with a type anywhere is NAMED (`a` shares `b`'s type).
    cb fn(a, b int) int
}

struct unnamed_cb {
    // A list with no typed entry is a list of unnamed parameter TYPES; a
    // declared type there resolves.
    cb fn(point) point
}

struct empty {}

type alias = point

type row = []int

struct box[t] {
    items []t
    table map[string]t
    maybe option[t]
}

struct pair[t] {
    b box[t]
}

fn id[t any](x t) t {
    return x
}

fn main() {
    var p point
    p.x = 1
    p.y = 2

    var a alias
    a.x = 3
    a.y = 4

    var e empty

    var r row
    r = append(r, 1)

    var b box[int]
    b.items = append(b.items, 2)

    var pr pair[int]
    pr.b.items = append(pr.b.items, 3)

    var o option[int]
    var s set[int]

    var ncb named_cb
    var ucb unnamed_cb

    var i int
    var u uint8
    var f float64
    var t bool
    var str string
    var sm u8
    var fl f32
    i = 5
    u = 6
    f = 7.5
    t = true
    str = "x"
    sm = 8
    fl = 9.5

    fmt.println(p.x + p.y + a.x + a.y)
    fmt.println(e)
    fmt.println(len(r))
    fmt.println(len(b.items))
    fmt.println(len(pr.b.items))
    fmt.println(o)
    fmt.println(len(s))
    fmt.println(id(i) + int(u) + int(sm))
    fmt.println(f + float64(fl))
    fmt.println(t)
    fmt.println(str)
    fmt.println(ncb.cb)
    fmt.println(ucb.cb)
}
