// ============================================================================
// unknown-type-name-reject — Checker corpus (issue #1583).
//
// PURPOSE: a NAMED type in a declaration that resolves to nothing. The type
// walker recursed through every constructor and matched the leaf with
// `TypeExpr::Named { .. } => {}`, so it never asked whether the name exists.
// Every shape below passed `hyb check` with 0 diagnostics and then failed
// `go build` with `undefined: missing`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE).
//
// ---- Why this is a REJECT ----
// Go has no type named `missing`, and Hybrid does not invent one: the
// emitter writes the name into the generated Go verbatim.
//   struct field / local var / parameter / result / alias target /
//   slice element / map key / map value / channel element / option payload /
//   array element / generic argument / explicit const type
// all emit `undefined: missing`.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `unknown-type-name-controls-accept.hy` pins every type name the language
// does allow: declared structs/aliases, generic parameters, Go's predeclared
// types and the prelude `option`/`set`.
// ============================================================================

struct broken_field {
    a missing
}

struct broken_fn_type {
    cb fn(missing)
    cb2 fn(int, missing) int
}

type broken_alias = missing

const broken_const missing = 1

fn broken_params(p missing) int {
    return 1
}

fn broken_result() missing {
    return 1
}

fn main() {
    var v broken_field
    fmt.println(v.a)

    var x missing
    fmt.println(x)

    var xs []missing
    fmt.println(len(xs))

    var mk map[missing]int
    fmt.println(len(mk))

    var mv map[string]missing
    fmt.println(len(mv))

    var c chan missing
    fmt.println(c)

    var o option[missing]
    fmt.println(o)

    var arr [2]missing
    fmt.println(len(arr))

    fmt.println(broken_params(1))
    fmt.println(broken_result())
    fmt.println(broken_const)
}
