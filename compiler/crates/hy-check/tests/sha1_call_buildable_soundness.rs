//! Soundness tests for crypto/sha1 package member call arity and non-callable constants (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of sha1 package functions validate exact parameter arity,
//! reject variadic spread on non-variadic functions, and that package constants (size, block_size)
//! are rejected fail-closed with HYB-E1002 when called as functions.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_sha1_constants_rejected_as_call_targets() {
    let src = r#"
package main

import "crypto/sha1"

fn main() {
    _ = sha1.size()
    _ = sha1.block_size()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 2, "expected 2 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_sha1_new_nullary_arity_rejected() {
    let bad1 = r#"
package main

import "crypto/sha1"

fn main() {
    _ = sha1.new("extra")
}
"#;
    let res1 = check(bad1);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert_eq!(codes(&res1).len(), 1);
    assert_eq!(codes(&res1)[0], "HYB-E1002");

    let bad2 = r#"
package main

import "crypto/sha1"

fn main() {
    _ = sha1.new(1, 2)
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert_eq!(codes(&res2).len(), 1);
    assert_eq!(codes(&res2)[0], "HYB-E1002");
}

#[test]
fn test_sha1_sum_unary_arity_rejected() {
    let bad0 = r#"
package main

import "crypto/sha1"

fn main() {
    _ = sha1.sum()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert_eq!(codes(&res0).len(), 1);
    assert_eq!(codes(&res0)[0], "HYB-E1002");

    let bad2 = r#"
package main

import "crypto/sha1"

fn test_it(data []byte) {
    _ = sha1.sum(data, "extra")
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert_eq!(codes(&res2).len(), 1);
    assert_eq!(codes(&res2)[0], "HYB-E1002");
}

#[test]
fn test_sha1_variadic_spread_rejected() {
    let bad_spread = r#"
package main

import "crypto/sha1"

fn test_it(b []byte) {
    _ = sha1.sum(b...)
}
"#;
    let res = check(bad_spread);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(codes(&res).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_sha1_valid_calls_and_references_accept() {
    let good = r#"
package main

import "crypto/sha1"

fn run(data []byte) {
    h := sha1.new()
    _ = h
    s := sha1.sum(data)
    _ = s
    _ = sha1.size
    _ = sha1.block_size
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
