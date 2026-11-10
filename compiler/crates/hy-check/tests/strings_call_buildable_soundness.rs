//! Soundness tests for strings package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of strings package functions validate exact parameter arity,
//! and that package types (builder, reader, replacer) are rejected fail-closed with HYB-E1002.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_strings_non_callable_types_rejected() {
    let src = r#"
package main

import "strings"

fn main() {
    _ = strings.builder(1)
    _ = strings.reader()
    _ = strings.replacer()
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
fn test_strings_unary_arity_rejected() {
    let bad0 = r#"
package main

import "strings"

fn main() {
    _ = strings.to_lower()
    _ = strings.trim_space()
    _ = strings.new_reader()
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

import "strings"

fn test_it(s string) {
    _ = strings.to_lower(s, "extra")
    _ = strings.trim_space(s, 1)
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
fn test_strings_binary_arity_rejected() {
    let bad_split0 = r#"
package main

import "strings"

fn main() {
    _ = strings.split()
}
"#;
    let res_s0 = check(bad_split0);
    assert_eq!(res_s0.verdict, Verdict::Reject);
    assert!(codes(&res_s0).contains(&"HYB-E1002".to_string()));

    let bad_contains1 = r#"
package main

import "strings"

fn test_it(s string) {
    _ = strings.contains(s)
}
"#;
    let res_c1 = check(bad_contains1);
    assert_eq!(res_c1.verdict, Verdict::Reject);
    assert!(codes(&res_c1).contains(&"HYB-E1002".to_string()));

    let bad_has_prefix3 = r#"
package main

import "strings"

fn test_it(s string, prefix string) {
    _ = strings.has_prefix(s, prefix, 1)
}
"#;
    let res_hp3 = check(bad_has_prefix3);
    assert_eq!(res_hp3.verdict, Verdict::Reject);
    assert!(codes(&res_hp3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_strings_split_n_and_replace_arity_rejected() {
    let bad_sn2 = r#"
package main

import "strings"

fn test_it(s string, sep string) {
    _ = strings.split_n(s, sep)
}
"#;
    let res_sn = check(bad_sn2);
    assert_eq!(res_sn.verdict, Verdict::Reject);
    assert!(codes(&res_sn).contains(&"HYB-E1002".to_string()));

    let bad_rep3 = r#"
package main

import "strings"

fn test_it(s string, old string, new string) {
    _ = strings.replace(s, old, new)
}
"#;
    let res_rep = check(bad_rep3);
    assert_eq!(res_rep.verdict, Verdict::Reject);
    assert!(codes(&res_rep).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_strings_valid_constructors_and_references_accept() {
    let good = r#"
package main

import "strings"

fn run(s string, sep string, old string, new string) {
    l := strings.to_lower(s)
    _ = l
    u := strings.to_upper(s)
    _ = u
    t := strings.trim_space(s)
    _ = t
    sp := strings.split(s, sep)
    _ = sp
    c := strings.contains(s, "hello")
    _ = c
    hp := strings.has_prefix(s, "http")
    _ = hp
    r := strings.new_reader(s)
    _ = r
    b := strings.builder()
    _ = b
    sn := strings.split_n(s, sep, 2)
    _ = sn
    rep := strings.replace(s, old, new, 1)
    _ = rep
    ra := strings.replace_all(s, old, new)
    _ = ra
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
