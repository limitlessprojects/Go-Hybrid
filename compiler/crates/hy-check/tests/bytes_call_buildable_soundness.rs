//! Soundness tests for bytes package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of bytes package functions validate exact parameter arity,
//! reject variadic spread on non-variadic functions, and that package types and variables
//! (buffer, reader, min_read, err_too_large) are rejected fail-closed with HYB-E1002 when called.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_bytes_non_callable_symbols_rejected() {
    let src = r#"
package main

import "bytes"

fn main() {
    _ = bytes.buffer()
    _ = bytes.reader()
    _ = bytes.min_read()
    _ = bytes.err_too_large()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 4, "expected 4 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_bytes_unary_arity_rejected() {
    let bad0 = r#"
package main

import "bytes"

fn main() {
    _ = bytes.to_lower()
    _ = bytes.trim_space()
    _ = bytes.new_buffer()
    _ = bytes.new_reader()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert_eq!(codes(&res0).len(), 4);
    for c in codes(&res0) {
        assert_eq!(c, "HYB-E1002");
    }

    let bad2 = r#"
package main

import "bytes"

fn test_it(b []byte) {
    _ = bytes.to_lower(b, b)
    _ = bytes.trim_space(b, 1)
    _ = bytes.new_buffer(b, 2)
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert_eq!(codes(&res2).len(), 3);
    for c in codes(&res2) {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_bytes_binary_arity_rejected() {
    let bad_equal0 = r#"
package main

import "bytes"

fn main() {
    _ = bytes.equal()
}
"#;
    let res_e0 = check(bad_equal0);
    assert_eq!(res_e0.verdict, Verdict::Reject);
    assert!(codes(&res_e0).contains(&"HYB-E1002".to_string()));

    let bad_split1 = r#"
package main

import "bytes"

fn test_it(b []byte) {
    _ = bytes.split(b)
}
"#;
    let res_s1 = check(bad_split1);
    assert_eq!(res_s1.verdict, Verdict::Reject);
    assert!(codes(&res_s1).contains(&"HYB-E1002".to_string()));

    let bad_contains3 = r#"
package main

import "bytes"

fn test_it(a []byte, b []byte) {
    _ = bytes.contains(a, b, 1)
}
"#;
    let res_c3 = check(bad_contains3);
    assert_eq!(res_c3.verdict, Verdict::Reject);
    assert!(codes(&res_c3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_bytes_ternary_and_quaternary_arity_rejected() {
    let bad_rep_all2 = r#"
package main

import "bytes"

fn test_it(s []byte, old []byte) {
    _ = bytes.replace_all(s, old)
}
"#;
    let res_ra = check(bad_rep_all2);
    assert_eq!(res_ra.verdict, Verdict::Reject);
    assert!(codes(&res_ra).contains(&"HYB-E1002".to_string()));

    let bad_split_n2 = r#"
package main

import "bytes"

fn test_it(s []byte, sep []byte) {
    _ = bytes.split_n(s, sep)
}
"#;
    let res_sn = check(bad_split_n2);
    assert_eq!(res_sn.verdict, Verdict::Reject);
    assert!(codes(&res_sn).contains(&"HYB-E1002".to_string()));

    let bad_replace3 = r#"
package main

import "bytes"

fn test_it(s []byte, old []byte, new []byte) {
    _ = bytes.replace(s, old, new)
}
"#;
    let res_rep = check(bad_replace3);
    assert_eq!(res_rep.verdict, Verdict::Reject);
    assert!(codes(&res_rep).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_bytes_variadic_spread_rejected() {
    let bad_spread = r#"
package main

import "bytes"

fn test_it(b []byte) {
    _ = bytes.trim_space(b...)
}
"#;
    let res = check(bad_spread);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(codes(&res).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_bytes_valid_calls_and_references_accept() {
    let good = r#"
package main

import "bytes"

fn run(a []byte, b []byte, old []byte, new []byte) {
    l := bytes.to_lower(a)
    _ = l
    u := bytes.to_upper(a)
    _ = u
    t := bytes.trim_space(a)
    _ = t
    c := bytes.clone(a)
    _ = c
    buf := bytes.new_buffer(a)
    _ = buf
    r := bytes.new_reader(a)
    _ = r
    eq := bytes.equal(a, b)
    _ = eq
    ef := bytes.equal_fold(a, b)
    _ = ef
    ct := bytes.contains(a, b)
    _ = ct
    sp := bytes.split(a, b)
    _ = sp
    sn := bytes.split_n(a, b, 2)
    _ = sn
    ra := bytes.replace_all(a, old, new)
    _ = ra
    rep := bytes.replace(a, old, new, 1)
    _ = rep
    _ = bytes.min_read
    _ = bytes.err_too_large
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
