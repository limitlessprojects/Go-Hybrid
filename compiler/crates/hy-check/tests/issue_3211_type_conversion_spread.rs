//! Regression test for issue #3211:
//! hy-check must reject variadic spread `...` in type conversions
//! (named, slice, array) with HYB-E1002, matching Go specification:
//! `invalid use of ... in conversion to <type>`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "issue_3211.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn test_named_string_conversion_spread_rejected() {
    let src = r#"
fn main() {
    xs := []byte{65, 66}
    s := string(xs...)
    _ = s
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    let msgs = error_messages(&res);
    assert!(
        msgs.iter().any(|m| m.contains("cannot use `...` in conversion to `string`")),
        "expected error about spread in conversion to string, got: {:?}",
        msgs
    );
}

#[test]
fn test_named_string_rune_conversion_spread_rejected() {
    let src = r#"
fn main() {
    rs := []rune{'a', 'b'}
    s := string(rs...)
    _ = s
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    let msgs = error_messages(&res);
    assert!(
        msgs.iter().any(|m| m.contains("cannot use `...` in conversion to `string`")),
        "expected error about spread in conversion to string, got: {:?}",
        msgs
    );
}

#[test]
fn test_user_type_alias_conversion_spread_rejected() {
    let src = r#"
type my_int = int

fn main() {
    xs := []int{1, 2}
    v := my_int(xs...)
    _ = v
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    let msgs = error_messages(&res);
    assert!(
        msgs.iter().any(|m| m.contains("cannot use `...` in conversion to `my_int`")),
        "expected error about spread in conversion to my_int, got: {:?}",
        msgs
    );
}

#[test]
fn test_slice_byte_conversion_spread_rejected() {
    let src = r#"
fn main() {
    s := "hello"
    b := []byte(s...)
    _ = b
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    let msgs = error_messages(&res);
    assert!(
        msgs.iter().any(|m| m.contains("cannot use `...` in conversion to `[]byte`")),
        "expected error about spread in conversion to []byte, got: {:?}",
        msgs
    );
}

#[test]
fn test_slice_rune_conversion_spread_rejected() {
    let src = r#"
fn main() {
    s := "hello"
    b := []rune(s...)
    _ = b
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    let msgs = error_messages(&res);
    assert!(
        msgs.iter().any(|m| m.contains("cannot use `...` in conversion to `[]rune`")),
        "expected error about spread in conversion to []rune, got: {:?}",
        msgs
    );
}

#[test]
fn test_array_conversion_spread_rejected() {
    let src = r#"
fn main() {
    xs := []int{1, 2, 3}
    a := [3]int(xs...)
    _ = a
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    let msgs = error_messages(&res);
    assert!(
        msgs.iter().any(|m| m.contains("cannot use `...` in conversion to")),
        "expected error about spread in conversion, got: {:?}",
        msgs
    );
}

#[test]
fn test_valid_conversions_without_spread_accepted() {
    let src = r#"
type my_int = int

fn main() {
    xs := []byte{65, 66}
    s := string(xs)
    b := []byte(s)
    v := my_int(42)
    _ = s
    _ = b
    _ = v
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "diags: {:?}", res.diagnostics);
}
