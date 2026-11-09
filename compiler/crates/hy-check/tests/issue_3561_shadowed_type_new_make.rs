//! Regression test for issue #3561:
//! [buildable] hy-check accepts a value-shadowed type name inside new(T): emitted Go resolves it to the shadowing variable
//!
//! Under Go lexical scoping rules and Row 13 (BUILDABLE):
//! When a local variable or parameter shadows a type name in an enclosing block,
//! referencing that identifier in a type position (such as the type argument of `new` or `make`,
//! or in an explicit type ascription) causes Go to resolve the identifier as the variable,
//! failing compilation with `<ident> is not a type`.
//! hy-check must reject value-shadowed type names with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet should parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// 1. Direct repro from #3561: local value shadows struct before new(T)
#[test]
fn test_new_shadowed_struct_type_rejects() {
    let src = r#"
package main
struct holder { n int }

fn main() {
    holder := 5
    p := new(holder)
    p.n = 7
    println(p.n, holder)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 2. Local value shadows type alias before make(T)
#[test]
fn test_make_shadowed_type_alias_rejects() {
    let src = r#"
package main
type mymap = map[string]int

fn main() {
    mymap := 42
    m := make(mymap)
    println(m, mymap)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 3. Local value shadows struct before make([]T, len)
#[test]
fn test_make_slice_of_shadowed_type_rejects() {
    let src = r#"
package main
struct holder { n int }

fn main() {
    holder := 5
    s := make([]holder, 4)
    println(len(s), holder)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 4. Local value shadows struct before var declaration `var x T`
#[test]
fn test_var_decl_shadowed_type_rejects() {
    let src = r#"
package main
struct holder { n int }

fn main() {
    holder := 5
    var x holder
    println(x.n, holder)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 5. Function parameter shadows struct before new(T)
#[test]
fn test_param_shadowed_type_rejects() {
    let src = r#"
package main
struct holder { n int }

fn test(holder int) {
    p := new(holder)
    p.n = 7
    println(p.n, holder)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 6. Predeclared primitive type shadowed before new(T)
#[test]
fn test_predeclared_primitive_shadowed_rejects() {
    let src = r#"
package main
fn main() {
    int := 5
    p := new(int)
    println(p, int)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 7. Legitimate unshadowed new(T) and make(T) must be accepted
#[test]
fn test_unshadowed_new_and_make_accepts() {
    let src = r#"
package main
struct holder { n int }
type mymap = map[string]int

fn main() {
    p := new(holder)
    p.n = 7
    m := make(mymap)
    s := make([]int, 4)
    println(p.n, len(m), len(s))
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert_eq!(error_codes(&r), Vec::<String>::new());
}

// 8. new(holder) occurring BEFORE holder := 5 (lexical scope order) must be accepted
#[test]
fn test_new_before_shadowing_variable_accepts() {
    let src = r#"
package main
struct holder { n int }

fn main() {
    p := new(holder)
    p.n = 7
    holder := 5
    println(p.n, holder)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert_eq!(error_codes(&r), Vec::<String>::new());
}
