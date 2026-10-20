//! addr_of_addressable.rs — tests for HYB-E1002 rejection of address-of (`&`)
//! operators on non-addressable expressions (literals, binary/unary ops, calls,
//! constants, top-level functions, string indexes, slice expressions, blank ident).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn rejects_addr_of_int_literal() {
    let src = "fn main() {\n    p := &123\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_string_literal() {
    let src = "fn main() {\n    p := &\"hello\"\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_bool_literal() {
    let src = "fn main() {\n    p := &true\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_binary_expr() {
    let src = "fn main() {\n    x := 1\n    p := &(x + 1)\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_unary_expr() {
    let src = "fn main() {\n    x := 1\n    p := &(-x)\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_call_result() {
    let src = "fn foo() int { return 1 }\nfn main() {\n    p := &foo()\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_file_const() {
    let src = "const my_const = 42\nfn main() {\n    p := &my_const\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_local_const() {
    let src = "fn main() {\n    const c = 42\n    p := &c\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_top_level_fn() {
    let src = "fn foo() {}\nfn main() {\n    p := &foo\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_string_index() {
    let src = "fn main() {\n    s := \"hello\"\n    p := &s[0]\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_slice_expr() {
    let src = "fn main() {\n    s := \"hello\"\n    p := &s[1:3]\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_addr_of_blank_ident() {
    let src = "fn main() {\n    p := &_\n    _ = p\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn accepts_valid_address_of() {
    let src = "\
struct point {
    x int
    y int
}

fn main() {
    x := 10
    p1 := &x
    _ = p1

    pt := point{x: 1, y: 2}
    p2 := &pt.x
    _ = p2

    s := []int{1, 2, 3}
    p3 := &s[0]
    _ = p3

    p4 := &point{x: 1, y: 2}
    _ = p4

    m := map[string]int{\"a\": 1}
    p5 := &m[\"a\"]
    _ = p5
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", r.diagnostics);
}
