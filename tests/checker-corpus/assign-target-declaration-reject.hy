// ============================================================================
// assign-target-declaration-reject — Checker corpus (issue #1574).
//
// PURPOSE: a store / increment / compound assignment whose TARGET is not a
// variable but a package-scope declaration: a function, an imported package,
// a predeclared builtin, or a type name. `hyb check` accepted every one of
// them with 0 diagnostics and `go build` failed.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE).
//
// ---- Why this is a REJECT ----
// Go has no variable behind these names:
//   f = 2      -> cannot assign to f (neither addressable nor a map index expression)
//   f++        -> invalid operation: f++ (non-numeric type func() int)
//   f += 1     -> cannot assign to f …
//   f, x = 1, 2-> cannot assign to f …
//   fmt = 1    -> use of package fmt not in selector
//   len = 1    -> len (built-in) must be called
//   holder = 1 -> holder (type) is not an expression
//   error = 1  -> error (type) is not an expression
// The names all resolve in the file (or the curated package table), so this
// is not an `Unknown` fail-open case.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `assign-target-declaration-controls-accept.hy` carries the legal targets:
// a local var, a parameter, `_`, and a local binding that SHADOWS a builtin.
// ============================================================================

struct holder {
    a int
}

fn f() int {
    return 1
}

fn main() {
    // a function declaration
    f = 2
    fmt.println(f)

    // the same place through `++` and a compound assignment
    f++
    f += 1

    // a multi-target assignment reaches the same check per left-hand name
    var x int
    f, x = 1, 2
    fmt.println(x)

    // an imported package prefix
    fmt = 1

    // a predeclared builtin and a predeclared type
    len = 1
    error = 1

    // a local struct type
    holder = 1
}
