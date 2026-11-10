//! Soundness tests for archive/tar package member call arity and complete 31-symbol table
//! including Type* and Format* families (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of tar package functions validate exact parameter arity,
//! reject variadic spread on non-variadic functions, and that package constants, error variables,
//! and struct/interface types are rejected fail-closed with HYB-E1002 when called as functions.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_tar_constants_types_and_vars_rejected_as_call_targets() {
    let src = r#"
package main

import "archive/tar"

fn main() {
    _ = tar.header()
    _ = tar.reader()
    _ = tar.writer()
    _ = tar.format()
    _ = tar.file_info_names()
    _ = tar.err_header()
    _ = tar.err_write_too_long()
    _ = tar.err_field_too_long()
    _ = tar.err_write_after_close()
    _ = tar.err_insecure_path()
    _ = tar.format_unknown()
    _ = tar.format_ustar()
    _ = tar.format_pax()
    _ = tar.format_gnu()
    _ = tar.type_reg()
    _ = tar.type_reg_a()
    _ = tar.type_link()
    _ = tar.type_symlink()
    _ = tar.type_char()
    _ = tar.type_block()
    _ = tar.type_dir()
    _ = tar.type_fifo()
    _ = tar.type_cont()
    _ = tar.type_x_header()
    _ = tar.type_x_global_header()
    _ = tar.type_gnu_long_name()
    _ = tar.type_gnu_long_link()
    _ = tar.type_gnu_sparse()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 28, "expected 28 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_tar_unary_arity_rejected() {
    let bad0 = r#"
package main

import "archive/tar"

fn main() {
    _ = tar.new_reader()
    _ = tar.new_writer()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert_eq!(codes(&res0).len(), 2);
    for c in codes(&res0) {
        assert_eq!(c, "HYB-E1002");
    }

    let bad2 = r#"
package main

import "archive/tar"

fn test_it(r any, w any) {
    _ = tar.new_reader(r, 1)
    _ = tar.new_writer(w, 2)
}
"#;
    let res2 = check(bad2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert_eq!(codes(&res2).len(), 2);
    for c in codes(&res2) {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_tar_binary_arity_rejected() {
    let bad0 = r#"
package main

import "archive/tar"

fn main() {
    _ = tar.file_info_header()
}
"#;
    let res0 = check(bad0);
    assert_eq!(res0.verdict, Verdict::Reject);
    assert_eq!(codes(&res0).len(), 1);
    assert_eq!(codes(&res0)[0], "HYB-E1002");

    let bad1 = r#"
package main

import "archive/tar"

fn test_it(fi any) {
    _ = tar.file_info_header(fi)
}
"#;
    let res1 = check(bad1);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert_eq!(codes(&res1).len(), 1);
    assert_eq!(codes(&res1)[0], "HYB-E1002");

    let bad3 = r#"
package main

import "archive/tar"

fn test_it(fi any, link string) {
    _ = tar.file_info_header(fi, link, "extra")
}
"#;
    let res3 = check(bad3);
    assert_eq!(res3.verdict, Verdict::Reject);
    assert_eq!(codes(&res3).len(), 1);
    assert_eq!(codes(&res3)[0], "HYB-E1002");
}

#[test]
fn test_tar_variadic_spread_rejected() {
    let bad_spread = r#"
package main

import "archive/tar"

fn test_it(fi any, link string) {
    _ = tar.file_info_header(fi, link...)
}
"#;
    let res = check(bad_spread);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(codes(&res).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_tar_valid_calls_and_references_accept() {
    let good = r#"
package main

import "archive/tar"

fn run(r any, w any, fi any) {
    tr := tar.new_reader(r)
    _ = tr
    tw := tar.new_writer(w)
    _ = tw
    h := tar.file_info_header(fi, "")
    _ = h
    _ = tar.format_gnu
    _ = tar.format_pax
    _ = tar.format_ustar
    _ = tar.format_unknown
    _ = tar.type_reg
    _ = tar.type_reg_a
    _ = tar.type_link
    _ = tar.type_symlink
    _ = tar.type_char
    _ = tar.type_block
    _ = tar.type_dir
    _ = tar.type_fifo
    _ = tar.type_cont
    _ = tar.type_x_header
    _ = tar.type_x_global_header
    _ = tar.type_gnu_long_name
    _ = tar.type_gnu_long_link
    _ = tar.type_gnu_sparse
    _ = tar.err_header
    _ = tar.err_insecure_path
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
