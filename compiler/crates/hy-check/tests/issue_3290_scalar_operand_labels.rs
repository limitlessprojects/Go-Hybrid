//! issue_3290_scalar_operand_labels.rs
//!
//! Regression test for issue #3290:
//! E1002 diagnostic messages must name the exact numeric types (e.g. `int32`, `int64`,
//! `float32`, `float64`, `uint8`) when known, rather than collapsing them to `int` and `float64`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("parse error");
    check_file(&ast)
}

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn equality_names_both_widths() {
    let src = r#"
package main

fn main() {
    var a int32 = 3
    var b int64 = 3
    _ = a == b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("found `int32` and `int64`")), "actual: {:?}", msgs);
    assert!(!msgs.iter().any(|m| m.contains("found `int` and `int`")), "actual: {:?}", msgs);
}

#[test]
fn addition_names_both_widths() {
    let src = r#"
package main

fn main() {
    var a int32 = 3
    var b int64 = 3
    _ = a + b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("types `int32` and `int64`")), "actual: {:?}", msgs);
    assert!(!msgs.iter().any(|m| m.contains("types `int` and `int`")), "actual: {:?}", msgs);
}

#[test]
fn ordered_and_arith_name_float_widths() {
    let src = r#"
package main

fn main() {
    var x float32 = 1.0
    var y float64 = 2.0
    _ = x < y
    _ = x - y
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("`float32` and `float64`")), "actual: {:?}", msgs);
    assert!(!msgs.iter().any(|m| m.contains("`float64` and `float64`")), "actual: {:?}", msgs);
}

#[test]
fn bitwise_names_signedness() {
    let src = r#"
package main

fn main() {
    var u uint8 = 1
    var v int8 = 2
    _ = u | v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("`uint8` and `int8`")), "actual: {:?}", msgs);
    assert!(!msgs.iter().any(|m| m.contains("`int` and `int`")), "actual: {:?}", msgs);
}

#[test]
fn plain_assignment_names_both_widths() {
    let src = r#"
package main

fn main() {
    var a float32 = 1.0
    var b float64 = 2.0
    a = b
    _ = a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("cannot assign a value of type `float64` to `a` (type `float32`)")), "actual: {:?}", msgs);
    assert!(!msgs.iter().any(|m| m.contains("to `a` (type `float64`)")), "actual: {:?}", msgs);
}

#[test]
fn compound_assignments_name_both_widths() {
    let src = r#"
package main

fn main() {
    var a int32 = 3
    var b int64 = 4
    a += b
    a -= b
    a *= b
    a %= b
    _ = a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    for m in &msgs {
        assert!(m.contains("int32") && m.contains("int64"), "actual: {:?}", m);
        assert!(!m.contains("`int` and `int`") && !m.contains("type `int`"), "actual: {:?}", m);
    }
}

#[test]
fn struct_literal_field_names_both_widths() {
    let src = r#"
package main

struct my_struct {
    f float32
}

fn main() {
    var x float64 = 1.0
    _ = my_struct{f: x}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("value of type `float64`") && m.contains("declared `float32`")), "actual: {:?}", msgs);
}

#[test]
fn fn_call_arg_names_both_widths() {
    let src = r#"
package main

fn foo(x int32) {}

fn main() {
    var v int64 = 10
    foo(v)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("value of type `int64`") && m.contains("as `int32` value in argument to `foo`")), "actual: {:?}", msgs);
}

#[test]
fn method_arg_names_both_widths() {
    let src = r#"
package main

struct my_receiver {}

fn (m my_receiver) bar(x int32) {}

fn main() {
    var m my_receiver
    var v int64 = 10
    m.bar(v)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("value of type `int64`") && m.contains("as `int32` value in argument to `my_receiver.bar`")), "actual: {:?}", msgs);
}

#[test]
fn kind_mismatch_keeps_kind_labels() {
    let src = r#"
package main

fn main() {
    var s string = "hello"
    var n int = 42
    _ = s - n
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let msgs = error_messages(&r);
    assert!(msgs.iter().any(|m| m.contains("not defined on `string` and `int`")), "actual: {:?}", msgs);
}
