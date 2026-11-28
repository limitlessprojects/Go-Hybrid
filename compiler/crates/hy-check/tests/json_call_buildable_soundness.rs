//! Soundness tests for encoding/json package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of encoding/json package functions validate exact parameter arity,
//! and that package types and errors are rejected fail-closed with HYB-E1002.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_json_non_callable_types_rejected() {
    let src = r#"
package main

import "encoding/json"

fn main() {
    _ = json.decoder()
    _ = json.encoder()
    _ = json.raw_message()
    _ = json.delim()
    _ = json.token()
    _ = json.number()
    _ = json.syntax_error()
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
fn test_json_marshal_and_unmarshal_arity_rejected() {
    let bad_m0 = r#"
package main

import "encoding/json"

fn main() {
    _ = json.marshal()
}
"#;
    let res_m0 = check(bad_m0);
    assert_eq!(res_m0.verdict, Verdict::Reject);
    assert!(codes(&res_m0).contains(&"HYB-E1002".to_string()));

    let bad_m2 = r#"
package main

import "encoding/json"

fn test_it(v any) {
    _ = json.marshal(v, 1)
}
"#;
    let res_m2 = check(bad_m2);
    assert_eq!(res_m2.verdict, Verdict::Reject);
    assert!(codes(&res_m2).contains(&"HYB-E1002".to_string()));

    let bad_u1 = r#"
package main

import "encoding/json"

fn test_it(data any) {
    _ = json.unmarshal(data)
}
"#;
    let res_u1 = check(bad_u1);
    assert_eq!(res_u1.verdict, Verdict::Reject);
    assert!(codes(&res_u1).contains(&"HYB-E1002".to_string()));

    let bad_u3 = r#"
package main

import "encoding/json"

fn test_it(data any, v any) {
    _ = json.unmarshal(data, v, 1)
}
"#;
    let res_u3 = check(bad_u3);
    assert_eq!(res_u3.verdict, Verdict::Reject);
    assert!(codes(&res_u3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_json_marshal_indent_and_indent_arity_rejected() {
    let bad_mi0 = r#"
package main

import "encoding/json"

fn main() {
    _ = json.marshal_indent()
}
"#;
    let res_mi0 = check(bad_mi0);
    assert_eq!(res_mi0.verdict, Verdict::Reject);
    assert!(codes(&res_mi0).contains(&"HYB-E1002".to_string()));

    let bad_i2 = r#"
package main

import "encoding/json"

fn test_it(dst any, src any) {
    _ = json.indent(dst, src)
}
"#;
    let res_i2 = check(bad_i2);
    assert_eq!(res_i2.verdict, Verdict::Reject);
    assert!(codes(&res_i2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_json_compact_and_html_escape_arity_rejected() {
    let bad_c1 = r#"
package main

import "encoding/json"

fn test_it(dst any) {
    _ = json.compact(dst)
}
"#;
    let res_c1 = check(bad_c1);
    assert_eq!(res_c1.verdict, Verdict::Reject);
    assert!(codes(&res_c1).contains(&"HYB-E1002".to_string()));

    let bad_h1 = r#"
package main

import "encoding/json"

fn test_it(dst any) {
    _ = json.html_escape(dst)
}
"#;
    let res_h1 = check(bad_h1);
    assert_eq!(res_h1.verdict, Verdict::Reject);
    assert!(codes(&res_h1).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_json_new_decoder_encoder_arity_rejected() {
    let bad_d0 = r#"
package main

import "encoding/json"

fn main() {
    _ = json.new_decoder()
}
"#;
    let res_d0 = check(bad_d0);
    assert_eq!(res_d0.verdict, Verdict::Reject);
    assert!(codes(&res_d0).contains(&"HYB-E1002".to_string()));

    let bad_e2 = r#"
package main

import "encoding/json"

fn test_it(w any) {
    _ = json.new_encoder(w, 1)
}
"#;
    let res_e2 = check(bad_e2);
    assert_eq!(res_e2.verdict, Verdict::Reject);
    assert!(codes(&res_e2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_json_valid_constructors_and_references_accept() {
    let good = r#"
package main

import "encoding/json"

fn run(v any, data any, dst any, src any, r any, w any) {
    m, _ := json.marshal(v)
    _ = m
    mi, _ := json.marshal_indent(v, "", "  ")
    _ = mi
    u := json.unmarshal(data, v)
    _ = u
    val := json.valid(data)
    _ = val
    c := json.compact(dst, src)
    _ = c
    json.html_escape(dst, src)
    ind := json.indent(dst, src, "", "  ")
    _ = ind
    dec := json.new_decoder(r)
    _ = dec
    enc := json.new_encoder(w)
    _ = enc
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
