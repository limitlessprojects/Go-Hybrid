//! Issue #3029: a function-local value (`:=` local, parameter, loop/arm
//! binding) named like a type (`int`, `string`, `bool`, `byte`, `any`, a user
//! struct) and then used as a type in the same scope. The emitter writes both
//! names verbatim, so Go reads the type position as the variable and the
//! module fails `go build` with `X (local variable) is not a type`. The
//! checker now rejects the type use with HYB-E1001. Every rejected program
//! below was `OK (0 diagnostics)` on main and failed go1.26.9 `go build`;
//! every accepted program builds.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {:?}", d));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for a type use shadowed by a local value, got {found:?}
{src}"
    );
}

fn accepts(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected no errors, got {found:?}
{src}"
    );
}

#[test]
fn rejects_local_int_then_var_decl() {
    rejects("fn main() {\n    int := 1\n    var y int = 2\n    fmt.println(int, y)\n}\n");
}

#[test]
fn rejects_local_string_then_var_decl() {
    rejects("fn main() {\n    string := \"a\"\n    var t string = \"b\"\n    fmt.println(string, t)\n}\n");
}

#[test]
fn rejects_local_any_then_var_decl() {
    rejects("fn main() {\n    any := 1\n    var x any = 2\n    fmt.println(any, x)\n}\n");
}

#[test]
fn rejects_param_int_then_var_decl() {
    rejects("fn f(int int) int {\n    var y int = int\n    return y\n}\nfn main() { fmt.println(f(1)) }\n");
}

#[test]
fn rejects_loop_binding_string_then_var_decl() {
    rejects("fn main() {\n    xs := []int{1}\n    for string in xs {\n        var t string = \"a\"\n        fmt.println(string, t)\n    }\n}\n");
}

#[test]
fn rejects_local_bool_then_slice_literal() {
    rejects("fn main() {\n    bool := 1\n    xs := []bool{true}\n    fmt.println(bool, xs)\n}\n");
}

#[test]
fn rejects_local_int_then_map_literal() {
    rejects("fn main() {\n    int := 1\n    m := map[string]int{\"a\": 2}\n    fmt.println(int, m)\n}\n");
}

#[test]
fn rejects_local_byte_then_conversion() {
    rejects("fn main() {\n    byte := 1\n    s := \"ab\"\n    bs := []byte(s)\n    fmt.println(byte, bs)\n}\n");
}

#[test]
fn rejects_local_struct_name_then_struct_literal() {
    rejects("struct p { x int }\nfn main() {\n    p := p{x: 1}\n    q := p{x: 2}\n    fmt.println(p, q)\n}\n");
}

#[test]
fn rejects_closure_param_type_reads_enclosing_local() {
    rejects("fn main() {\n    int := 1\n    f := fn(x int) int { return x }\n    fmt.println(int, f(2))\n}\n");
}

#[test]
fn rejects_closure_local_shadow_then_var_decl() {
    rejects("fn main() {\n    f := fn() {\n        string := \"a\"\n        var t string = \"b\"\n        fmt.println(string, t)\n    }\n    f()\n}\n");
}

#[test]
fn accepts_inner_block_shadow_then_type_after_block() {
    accepts("fn main() {\n    x := 1\n    if x > 0 {\n        int := 2\n        fmt.println(int)\n    }\n    var y int = 3\n    fmt.println(y)\n}\n");
}

#[test]
fn accepts_shadow_only_value_use() {
    accepts("fn main() {\n    main := 1\n    fmt.println(main)\n}\n");
}

#[test]
fn accepts_param_named_int_value_use_only() {
    accepts("fn f(int int) int { return int }\nfn main() { fmt.println(f(1)) }\n");
}

#[test]
fn accepts_local_named_error_value_use_only() {
    accepts("fn main() {\n    error := \"x\"\n    fmt.println(error)\n    e := errors.new(\"y\")\n    fmt.println(e)\n}\n");
}

#[test]
fn accepts_user_struct_named_int() {
    accepts("struct int { v i64 }\nfn main() {\n    a := int{v: 1}\n    fmt.println(a.v)\n}\n");
}

#[test]
fn accepts_spec_spelling_f64_renamed_by_emitter() {
    accepts("fn main() {\n    f64 := 1\n    var g f64 = 2.0\n    fmt.println(f64, g)\n}\n");
}

#[test]
fn accepts_local_error_with_foreign_err() {
    accepts("fn main() {\n    error := \"x\"\n    fmt.println(error)\n    _, err := strconv.atoi(\"1\")\n    fmt.println(err)\n}\n");
}

#[test]
fn accepts_local_named_panic() {
    accepts("fn main() {\n    panic := 1\n    fmt.println(panic)\n}\n");
}

#[test]
fn accepts_struct_literal_binding_same_name_first_use() {
    accepts("struct p {\n    x int\n}\nfn main() {\n    p := p{x: 1}\n    fmt.println(p.x)\n}\n");
}

#[test]
fn accepts_closure_param_named_int() {
    accepts("fn main() {\n    f := fn(int int) int { return int + 1 }\n    var y int = f(1)\n    fmt.println(y)\n}\n");
}
