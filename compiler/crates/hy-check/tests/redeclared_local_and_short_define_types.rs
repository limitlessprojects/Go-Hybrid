//! redeclared_local_and_short_define_types.rs
//! Regression tests for local variable redeclaration and short-define (`:=`)
//! type mutation soundness (HYB-E1002, rule BUILDABLE).
//!
//! Go spec rules enforced:
//! 1. In a given block, an identifier may only be declared once; local `var`,
//!    `const`, or `let` cannot redeclare an existing identifier in the same block.
//! 2. In a short variable declaration (`:=`), redeclaration of an existing variable
//!    acts as an assignment and must match the variable's existing declared type.
//! 3. Outer block shadowing in inner blocks remains legal with any type.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(r: &hy_check::CheckResult, msg_substr: &str) {
    let has_e1002 = r
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == "HYB-E1002" && d.message.contains(msg_substr));
    assert!(
        has_e1002,
        "expected HYB-E1002 containing {:?}, got diagnostics: {:?}",
        msg_substr, r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// 1. Short-define (:=) type mutation tests
// ---------------------------------------------------------------------------

#[test]
fn same_scope_short_define_type_mutation_rejected() {
    let src = r#"package main

fn main() {
    x := 1
    x, y := "hello", 2
    _ = x
    _ = y
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "cannot assign a value of type `string` to redeclared variable `x` (type `int`)");
}

#[test]
fn same_scope_short_define_same_type_accepted() {
    let src = r#"package main

fn main() {
    x := 1
    x, y := 2, 3
    _ = x
    _ = y
}
"#;
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "same-type redeclaration is valid Go, got: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn same_scope_short_define_multi_value_call_type_mutation_rejected() {
    let src = r#"package main

fn pair() (string, int) {
    return "ok", 1
}

fn main() {
    x := 1
    x, y := pair()
    _ = x
    _ = y
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "cannot assign a value of type `string` to redeclared variable `x` (type `int`)");
}

#[test]
fn same_scope_short_define_multi_value_call_same_type_accepted() {
    let src = r#"package main

fn pair() (int, string) {
    return 1, "ok"
}

fn main() {
    x := 1
    x, s := pair()
    _ = x
    _ = s
}
"#;
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "same-type redeclaration from call is valid Go, got: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn same_scope_short_define_map_comma_ok_type_mutation_rejected() {
    let src = r#"package main

fn main() {
    var m map[string]int
    s := "hello"
    s, ok := m["key"]
    _ = s
    _ = ok
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "cannot assign a value of type `int` to redeclared variable `s` (type `string`)");
}

#[test]
fn same_scope_short_define_chan_comma_ok_type_mutation_rejected() {
    let src = r#"package main

fn main() {
    var ch chan int
    s := "hello"
    s, ok := <-ch
    _ = s
    _ = ok
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "cannot assign a value of type `int` to redeclared variable `s` (type `string`)");
}

// ---------------------------------------------------------------------------
// 2. Same-scope local redeclaration tests (var, const, let)
// ---------------------------------------------------------------------------

#[test]
fn same_scope_var_redeclaration_rejected() {
    let src = r#"package main

fn main() {
    var a int = 1
    var a int = 2
    _ = a
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "`a` redeclared in this block");
}

#[test]
fn same_scope_var_redeclaration_no_init_rejected() {
    let src = r#"package main

fn main() {
    var a int
    var a int
    _ = a
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "`a` redeclared in this block");
}

#[test]
fn param_redeclaration_by_var_rejected() {
    let src = r#"package main

fn foo(x int) {
    var x int = 1
    _ = x
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "`x` redeclared in this block");
}

#[test]
fn param_short_define_type_mutation_rejected() {
    let src = r#"package main

fn foo(x int) {
    x, y := "hello", 1
    _ = x
    _ = y
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "cannot assign a value of type `string` to redeclared variable `x` (type `int`)");
}

#[test]
fn param_short_define_same_type_accepted() {
    let src = r#"package main

fn foo(x int) {
    x, y := 42, 1
    _ = x
    _ = y
}
"#;
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "same-type param redeclaration via := is valid Go, got: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// 3. Inner scope shadowing and blanks
// ---------------------------------------------------------------------------

#[test]
fn inner_scope_shadowing_different_type_accepted() {
    let src = r#"package main

fn main() {
    x := 1
    if true {
        x := "hello"
        _ = x
    }
    _ = x
}
"#;
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "inner block shadowing is valid Go, got: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn inner_scope_short_define_type_mutation_rejected() {
    let src = r#"package main

fn main() {
    x := 1
    if true {
        s := "hello"
        s, y := 123, 4
        _ = s
        _ = y
    }
    _ = x
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "cannot assign a value of type `int` to redeclared variable `s` (type `string`)");
}

#[test]
fn same_scope_const_redeclaration_rejected() {
    let src = r#"package main

fn main() {
    x := 1
    const x = 2
    _ = x
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "`x` redeclared in this block");
}

#[test]
fn blank_identifier_redecl_accepted() {
    let src = r#"package main

fn main() {
    var _ int = 1
    var _ string = "hello"
}
"#;
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "blank identifier _ can be declared multiple times, got: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
