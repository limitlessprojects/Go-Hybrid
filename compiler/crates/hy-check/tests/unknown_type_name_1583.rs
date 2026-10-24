//! unknown_type_name_1583.rs — a named type in a declaration that resolves to
//! nothing (issue #1583).
//!
//! `check_type_expr` walked every type constructor and stopped at the leaf
//! with `TypeExpr::Named { .. } => {}`, so it never asked whether the name
//! exists. Fourteen declaration positions therefore passed `hyb check` with
//! 0 diagnostics and then failed `go build` with `undefined: missing`
//! (GUARANTEES row 13, BUILDABLE):
//!
//!   struct s { a missing } / var x missing / fn f(p missing) /
//!   fn f() missing / type t = missing / []missing / map[missing]int /
//!   map[string]missing / chan missing / option[missing] / [2]missing /
//!   box[missing] / const c missing = 1 / struct s { cb fn(missing) }
//!
//! The check resolves the BASE name (generic arguments are folded into the
//! name string by the parser) and recurses into generic arguments. It must
//! keep failing OPEN on everything the language legitimately allows:
//! declared types (including zero-field structs), generic parameters,
//! Go's predeclared types, the SPEC conversion spellings, the prelude
//! types (`option`, `set`), and dotted foreign names.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_unknown_type(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {cs:?}"
    );
    let hit = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .expect("diagnostic present");
    assert!(
        hit.message.contains("unknown type `missing`"),
        "expected the unknown-type message for {what}, got {:?}",
        hit.message
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {what}");
}

fn assert_accepts(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(cs.is_empty(), "expected ACCEPT for {what}, got {cs:?}");
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {what}");
}

// ---------------------------------------------------------------------------
// Reject shapes — one per declaration position.
// ---------------------------------------------------------------------------

#[test]
fn struct_field_type_rejects() {
    assert_unknown_type(
        "struct s {\n    a missing\n}\n\nfn main() {\n    var v s\n    fmt.println(v.a)\n}\n",
        "a struct field of type `missing`",
    );
}

#[test]
fn local_var_type_rejects() {
    assert_unknown_type(
        "fn main() {\n    var x missing\n    fmt.println(x)\n}\n",
        "`var x missing`",
    );
}

#[test]
fn param_type_rejects() {
    assert_unknown_type(
        "fn f(p missing) int {\n    return 1\n}\n\nfn main() {\n    fmt.println(\"hi\")\n}\n",
        "a parameter of type `missing`",
    );
}

#[test]
fn result_type_rejects() {
    assert_unknown_type(
        "fn f() missing {\n    return 1\n}\n\nfn main() {\n    fmt.println(f())\n}\n",
        "a result of type `missing`",
    );
}

#[test]
fn alias_target_rejects() {
    assert_unknown_type(
        "type t = missing\n\nfn main() {\n    var v t\n    fmt.println(v)\n}\n",
        "`type t = missing`",
    );
}

#[test]
fn nested_container_elements_reject() {
    // Every constructor the walker recurses through must forward the check.
    assert_unknown_type(
        "fn main() {\n    var xs []missing\n    fmt.println(len(xs))\n}\n",
        "`[]missing`",
    );
    assert_unknown_type(
        "fn main() {\n    var m map[missing]int\n    fmt.println(len(m))\n}\n",
        "`map[missing]int` (key)",
    );
    assert_unknown_type(
        "fn main() {\n    var m map[string]missing\n    fmt.println(len(m))\n}\n",
        "`map[string]missing` (value)",
    );
    assert_unknown_type(
        "fn main() {\n    var c chan missing\n    fmt.println(c)\n}\n",
        "`chan missing`",
    );
    assert_unknown_type(
        "fn main() {\n    var o option[missing]\n    fmt.println(o)\n}\n",
        "`option[missing]`",
    );
    assert_unknown_type(
        "fn main() {\n    var a [2]missing\n    fmt.println(len(a))\n}\n",
        "`[2]missing`",
    );
}

#[test]
fn const_type_rejects() {
    assert_unknown_type(
        "const c missing = 1\n\nfn main() {\n    fmt.println(c)\n}\n",
        "`const c missing = 1`",
    );
}

#[test]
fn generic_argument_rejects() {
    assert_unknown_type(
        "struct box[t] {\n    v t\n}\n\nfn main() {\n    var b box[missing]\n    fmt.println(b.v)\n}\n",
        "`box[missing]` (the base resolves, the argument does not)",
    );
}

#[test]
fn fn_type_parameter_list_rejects() {
    // A `fn(...)` TYPE parameter list holds types, but `parse_params` parks a
    // bare ident in `name` with the empty-name sentinel in `ty`. Go's rule
    // decides: a list with no typed entry is a list of unnamed parameter types.
    assert_unknown_type(
        "struct s {\n    cb fn(missing)\n}\n\nfn main() {\n    var v s\n    fmt.println(v.cb)\n}\n",
        "`fn(missing)` (a single unnamed parameter type)",
    );
    assert_unknown_type(
        "struct s {\n    cb fn(int, missing) int\n}\n\nfn main() {\n    var v s\n    fmt.println(v.cb)\n}\n",
        "`fn(int, missing) int` (a mixed list of unnamed parameter types)",
    );
}

// ---------------------------------------------------------------------------
// Controls — everything the language legitimately accepts.
// ---------------------------------------------------------------------------

#[test]
fn declared_types_accept() {
    let src = "struct s {\n    a int\n}\n\ntype alias = s\n\nstruct empty {}\n\nfn main() {\n    var v s\n    v.a = 1\n    var w alias\n    w.a = 2\n    var e empty\n    fmt.println(v.a + w.a)\n    fmt.println(e)\n}\n";
    assert_accepts(src, "a declared struct, alias and zero-field struct");
}

#[test]
fn generic_parameters_accept() {
    // A struct type parameter in every nested position, a generic argument
    // that is itself a type parameter, and a generic fn parameter.
    let src = "struct box[t] {\n    items []t\n    table map[string]t\n    maybe option[t]\n}\n\nstruct pair[t] {\n    b box[t]\n}\n\nfn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    var b box[int]\n    b.items = append(b.items, 1)\n    fmt.println(len(b.items))\n    var p pair[int]\n    p.b.items = append(p.b.items, 2)\n    fmt.println(len(p.b.items))\n    fmt.println(id(3))\n}\n";
    assert_accepts(src, "generic struct / fn type parameters");
}

#[test]
fn predeclared_and_conversion_types_accept() {
    let src = "fn main() {\n    var a int\n    var b uint8\n    var c float64\n    var d bool\n    var e string\n    var f u8\n    var g f32\n    var h uintptr\n    var i any\n    a = 1\n    b = 2\n    c = 3.5\n    d = true\n    e = \"x\"\n    f = 4\n    g = 5.5\n    h = 6\n    i = 7\n    fmt.println(a + b)\n    fmt.println(c)\n    fmt.println(d)\n    fmt.println(e)\n    fmt.println(f + g)\n    fmt.println(h)\n    fmt.println(i)\n}\n";
    assert_accepts(src, "Go's predeclared types and the SPEC conversion spellings");
}

#[test]
fn prelude_types_accept() {
    assert_accepts(
        "fn main() {\n    var o option[int]\n    fmt.println(o)\n    var s set[int]\n    fmt.println(len(s))\n}\n",
        "the prelude types `option` and `set`",
    );
}

#[test]
fn dotted_foreign_type_accepts() {
    // Dotted names are foreign/Go-side; the foreign tables own them.
    assert_accepts(
        "import \"rust/serde_json\"\n\nstruct holder {\n    v serde_json.value\n}\n\nfn main() {\n    var h holder\n    fmt.println(h.v)\n}\n",
        "`serde_json.value`",
    );
}

#[test]
fn named_fn_type_parameter_accepts() {
    assert_accepts(
        "struct s {\n    cb fn(a int) int\n}\n\nfn main() {\n    var v s\n    fmt.println(v.cb)\n}\n",
        "a `fn(a int) int` field type",
    );
    // Go's rule: one typed entry makes the whole list named, so the bare
    // `a` is a parameter NAME sharing `b`'s type — not an unknown type.
    assert_accepts(
        "struct s {\n    cb fn(a, b int) int\n}\n\nfn main() {\n    var v s\n    fmt.println(v.cb)\n}\n",
        "a `fn(a, b int) int` shared-name parameter list",
    );
    // A single unnamed parameter that happens to be a DECLARED type must keep
    // passing: the name resolves, so the check is silent.
    assert_accepts(
        "struct point {\n    x int\n}\n\nstruct s {\n    cb fn(point) point\n}\n\nfn main() {\n    var v s\n    fmt.println(v.cb)\n}\n",
        "`fn(point) point` (an unnamed parameter of a declared type)",
    );
    // ...and a generic parameter in that position resolves too.
    assert_accepts(
        "struct box[t] {\n    cb fn(t) t\n}\n\nfn main() {\n    var b box[int]\n    fmt.println(b.cb)\n}\n",
        "`fn(t) t` inside `struct box[t]`",
    );
}
