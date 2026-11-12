//! Soundness tests for archive/zip package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of archive/zip package functions validate exact parameter arity,
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
fn test_zip_non_callable_variables_rejected() {
    let src = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.err_format()
    _ = zip.err_algorithm()
    _ = zip.err_checksum()
    _ = zip.err_insecure_path()
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
fn test_zip_variadic_spread_rejected() {
    let src = r#"
package main

import "archive/zip"

fn test_spread(w any, args []any) {
    _ = zip.new_writer(args...)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(codes(&res).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zip_non_callable_constants_and_types_rejected() {
    let src = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.deflate()
    _ = zip.store()
    _ = zip.reader()
    _ = zip.writer()
    _ = zip.read_closer()
    _ = zip.file_header()
    _ = zip.file()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 7, "expected 7 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_zip_new_reader_arity_rejected() {
    let bad0 = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.new_reader()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert!(codes(&res0).contains(&"HYB-E1002".to_string()));

    let bad1 = r#"
package main

import "archive/zip"

fn test_it(r any) {
    _ = zip.new_reader(r)
}
"#;
    let res1 = check(bad1);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(codes(&res1).contains(&"HYB-E1002".to_string()));

    let bad3 = r#"
package main

import "archive/zip"

fn test_it(r any) {
    _ = zip.new_reader(r, 1024, 0)
}
"#;
    let res3 = check(bad3);
    assert_eq!(res3.verdict, Verdict::Reject);
    assert!(codes(&res3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zip_new_writer_arity_rejected() {
    let bad0 = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.new_writer()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert!(codes(&res0).contains(&"HYB-E1002".to_string()));

    let bad2 = r#"
package main

import "archive/zip"

fn test_it(w any) {
    _ = zip.new_writer(w, 1)
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(codes(&res2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zip_open_reader_and_file_info_header_arity_rejected() {
    let bad_open0 = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.open_reader()
}
"#;
    let res_o = check(bad_open0);
    assert_eq!(res_o.verdict, Verdict::Reject);
    assert!(codes(&res_o).contains(&"HYB-E1002".to_string()));

    let bad_fi0 = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.file_info_header()
}
"#;
    let res_fi = check(bad_fi0);
    assert_eq!(res_fi.verdict, Verdict::Reject);
    assert!(codes(&res_fi).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zip_register_compressor_decompressor_arity_rejected() {
    let bad_c = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.register_compressor(1)
}
"#;
    let res_c = check(bad_c);
    assert_eq!(res_c.verdict, Verdict::Reject);
    assert!(codes(&res_c).contains(&"HYB-E1002".to_string()));

    let bad_d = r#"
package main

import "archive/zip"

fn main() {
    _ = zip.register_decompressor(1)
}
"#;
    let res_d = check(bad_d);
    assert_eq!(res_d.verdict, Verdict::Reject);
    assert!(codes(&res_d).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_zip_valid_constructors_and_references_accept() {
    let good = r#"
package main

import "archive/zip"

fn run(r any, w any, name string, fi any, method uint16, comp any, dcomp any) {
    reader := zip.new_reader(r, 4096)
    _ = reader
    writer := zip.new_writer(w)
    _ = writer
    opened := zip.open_reader(name)
    _ = opened
    header := zip.file_info_header(fi)
    _ = header
    zip.register_compressor(method, comp)
    zip.register_decompressor(method, dcomp)
    err1 := zip.err_format
    _ = err1
    err2 := zip.err_algorithm
    _ = err2
    err3 := zip.err_checksum
    _ = err3
    c1 := zip.deflate
    _ = c1
    c2 := zip.store
    _ = c2
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
