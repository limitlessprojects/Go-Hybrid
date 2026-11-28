//! Soundness tests for hash/crc32 package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of crc32 package functions validate exact parameter arity,
//! reject variadic spread on non-variadic functions, and that package constants, variables and types
//! (size, ieee, castagnoli, koopman, ieee_table, table) are rejected fail-closed with HYB-E1002 when called.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_crc32_non_callable_symbols_rejected() {
    let src = r#"
package main

import "hash/crc32"

fn main() {
    _ = crc32.size()
    _ = crc32.ieee()
    _ = crc32.castagnoli()
    _ = crc32.koopman()
    _ = crc32.ieee_table()
    _ = crc32.table()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 6, "expected 6 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_crc32_nullary_arity_rejected() {
    let bad = r#"
package main

import "hash/crc32"

fn main() {
    _ = crc32.new_ieee(123)
}
"#;
    let res = check(bad);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(codes(&res).len(), 1);
    assert_eq!(codes(&res)[0], "HYB-E1002");
}

#[test]
fn test_crc32_unary_arity_rejected() {
    let bad0 = r#"
package main

import "hash/crc32"

fn main() {
    _ = crc32.checksum_ieee()
    _ = crc32.new()
    _ = crc32.make_table()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert_eq!(codes(&res0).len(), 3);
    for c in codes(&res0) {
        assert_eq!(c, "HYB-E1002");
    }

    let bad2 = r#"
package main

import "hash/crc32"

fn test_it(v any) {
    _ = crc32.checksum_ieee(v, 1)
    _ = crc32.new(v, 2)
    _ = crc32.make_table(v, 3)
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
fn test_crc32_binary_arity_rejected() {
    let bad_checksum0 = r#"
package main

import "hash/crc32"

fn main() {
    _ = crc32.checksum()
}
"#;
    let res0 = check(bad_checksum0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert!(codes(&res0).contains(&"HYB-E1002".to_string()));

    let bad_checksum1 = r#"
package main

import "hash/crc32"

fn test_it(data []byte) {
    _ = crc32.checksum(data)
}
"#;
    let res1 = check(bad_checksum1);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(codes(&res1).contains(&"HYB-E1002".to_string()));

    let bad_checksum3 = r#"
package main

import "hash/crc32"

fn test_it(data []byte, t any) {
    _ = crc32.checksum(data, t, 1)
}
"#;
    let res3 = check(bad_checksum3);
    assert_eq!(res3.verdict, Verdict::Reject);
    assert!(codes(&res3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_crc32_ternary_arity_rejected() {
    let bad_update2 = r#"
package main

import "hash/crc32"

fn test_it(crc u32, t any) {
    _ = crc32.update(crc, t)
}
"#;
    let res2 = check(bad_update2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(codes(&res2).contains(&"HYB-E1002".to_string()));

    let bad_update4 = r#"
package main

import "hash/crc32"

fn test_it(crc u32, t any, p []byte) {
    _ = crc32.update(crc, t, p, 99)
}
"#;
    let res4 = check(bad_update4);
    assert_eq!(res4.verdict, Verdict::Reject);
    assert!(codes(&res4).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_crc32_variadic_spread_rejected() {
    let bad_spread = r#"
package main

import "hash/crc32"

fn test_it(b []byte) {
    _ = crc32.checksum_ieee(b...)
}
"#;
    let res = check(bad_spread);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(codes(&res).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_crc32_valid_calls_and_references_accept() {
    let good = r#"
package main

import "hash/crc32"

fn run(data []byte, poly u32) {
    t := crc32.make_table(poly)
    _ = t
    h1 := crc32.new(t)
    _ = h1
    h2 := crc32.new_ieee()
    _ = h2
    c1 := crc32.checksum_ieee(data)
    _ = c1
    c2 := crc32.checksum(data, t)
    _ = c2
    c3 := crc32.update(c2, t, data)
    _ = c3
    _ = crc32.size
    _ = crc32.ieee
    _ = crc32.castagnoli
    _ = crc32.koopman
    _ = crc32.ieee_table
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
