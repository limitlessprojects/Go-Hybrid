//! Soundness tests for compress/zlib package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of compress/zlib package functions validate exact parameter arity,
//! and that package error variables, compression constants, and types are rejected fail-closed with HYB-E1002.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_zlib_non_callable_variables_rejected() {
    let src = r#"
package main

import "compress/zlib"

fn main() {
    _ = zlib.err_checksum()
    _ = zlib.err_dictionary()
    _ = zlib.err_header()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 3, "expected 3 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_zlib_variadic_spread_rejected() {
    let src = r#"
package main

import "compress/zlib"

fn test_spread(w any, args []any) {
    _ = zlib.new_writer(args...)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(codes(&res).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zlib_non_callable_constants_and_types_rejected() {
    let src = r#"
package main

import "compress/zlib"

fn main() {
    _ = zlib.no_compression()
    _ = zlib.best_speed()
    _ = zlib.best_compression()
    _ = zlib.default_compression()
    _ = zlib.huffman_only()
    _ = zlib.writer()
    _ = zlib.reader()
    _ = zlib.resetter()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 8, "expected 8 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_zlib_new_reader_arity_rejected() {
    let bad0 = r#"
package main

import "compress/zlib"

fn main() {
    _ = zlib.new_reader()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert!(codes(&res0).contains(&"HYB-E1002".to_string()));

    let bad2 = r#"
package main

import "compress/zlib"

fn test_it(r any) {
    _ = zlib.new_reader(r, 1)
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(codes(&res2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zlib_new_writer_arity_rejected() {
    let bad0 = r#"
package main

import "compress/zlib"

fn main() {
    _ = zlib.new_writer()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert!(codes(&res0).contains(&"HYB-E1002".to_string()));

    let bad2 = r#"
package main

import "compress/zlib"

fn test_it(w any) {
    _ = zlib.new_writer(w, 1)
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(codes(&res2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zlib_new_reader_dict_arity_rejected() {
    let bad0 = r#"
package main

import "compress/zlib"

fn main() {
    _ = zlib.new_reader_dict()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert!(codes(&res0).contains(&"HYB-E1002".to_string()));

    let bad1 = r#"
package main

import "compress/zlib"

fn test_it(r any) {
    _ = zlib.new_reader_dict(r)
}
"#;
    let res1 = check(bad1);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(codes(&res1).contains(&"HYB-E1002".to_string()));

    let bad3 = r#"
package main

import "compress/zlib"

fn test_it(r any) {
    _ = zlib.new_reader_dict(r, 1, 2)
}
"#;
    let res3 = check(bad3);
    assert_eq!(res3.verdict, Verdict::Reject);
    assert!(codes(&res3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zlib_new_writer_level_arity_rejected() {
    let bad1 = r#"
package main

import "compress/zlib"

fn test_it(w any) {
    _ = zlib.new_writer_level(w)
}
"#;
    let res1 = check(bad1);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(codes(&res1).contains(&"HYB-E1002".to_string()));

    let bad3 = r#"
package main

import "compress/zlib"

fn test_it(w any) {
    _ = zlib.new_writer_level(w, 1, 2)
}
"#;
    let res3 = check(bad3);
    assert_eq!(res3.verdict, Verdict::Reject);
    assert!(codes(&res3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zlib_new_writer_level_dict_arity_rejected() {
    let bad2 = r#"
package main

import "compress/zlib"

fn test_it(w any) {
    _ = zlib.new_writer_level_dict(w, 1)
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(codes(&res2).contains(&"HYB-E1002".to_string()));

    let bad4 = r#"
package main

import "compress/zlib"

fn test_it(w any) {
    _ = zlib.new_writer_level_dict(w, 1, 2, 3)
}
"#;
    let res4 = check(bad4);
    assert_eq!(res4.verdict, Verdict::Reject);
    assert!(codes(&res4).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zlib_valid_constructors_and_references_accept() {
    let good = r#"
package main

import "compress/zlib"

fn run(r any, w any, dict any) {
    reader := zlib.new_reader(r)
    _ = reader
    writer := zlib.new_writer(w)
    _ = writer
    r_dict := zlib.new_reader_dict(r, dict)
    _ = r_dict
    w_level := zlib.new_writer_level(w, 9)
    _ = w_level
    w_level_dict := zlib.new_writer_level_dict(w, 9, dict)
    _ = w_level_dict

    err1 := zlib.err_checksum
    _ = err1
    err2 := zlib.err_dictionary
    _ = err2
    err3 := zlib.err_header
    _ = err3

    c0 := zlib.no_compression
    _ = c0
    c1 := zlib.best_speed
    _ = c1
    c2 := zlib.best_compression
    _ = c2
    c3 := zlib.default_compression
    _ = c3
    c4 := zlib.huffman_only
    _ = c4
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
