//! Soundness tests for encoding/xml package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of xml package functions validate exact parameter arity,
//! reject variadic spread on non-variadic functions, and that package types, variables and constants
//! (header, html_auto_close, decoder, encoder, name, attr, start_element) are rejected fail-closed with HYB-E1002 when called.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_xml_non_callable_symbols_rejected() {
    let src = r#"
package main

import "encoding/xml"

fn main() {
    _ = xml.header()
    _ = xml.html_auto_close()
    _ = xml.decoder()
    _ = xml.encoder()
    _ = xml.name()
    _ = xml.attr()
    _ = xml.start_element()
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
fn test_xml_unary_arity_rejected() {
    let bad0 = r#"
package main

import "encoding/xml"

fn main() {
    _ = xml.new_decoder()
    _ = xml.new_encoder()
    _ = xml.marshal()
    _ = xml.copy_token()
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

import "encoding/xml"

fn test_it(v any) {
    _ = xml.new_decoder(v, 1)
    _ = xml.marshal(v, 2)
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
fn test_xml_binary_arity_rejected() {
    let bad_unmarshal0 = r#"
package main

import "encoding/xml"

fn main() {
    _ = xml.unmarshal()
}
"#;
    let res_u0 = check(bad_unmarshal0);
    assert_eq!(res_u0.verdict, Verdict::Reject);
    assert!(codes(&res_u0).contains(&"HYB-E1002".to_string()));

    let bad_unmarshal1 = r#"
package main

import "encoding/xml"

fn test_it(data []byte) {
    _ = xml.unmarshal(data)
}
"#;
    let res_u1 = check(bad_unmarshal1);
    assert_eq!(res_u1.verdict, Verdict::Reject);
    assert!(codes(&res_u1).contains(&"HYB-E1002".to_string()));

    let bad_escape3 = r#"
package main

import "encoding/xml"

fn test_it(w any, s []byte) {
    _ = xml.escape_text(w, s, 1)
}
"#;
    let res_e3 = check(bad_escape3);
    assert_eq!(res_e3.verdict, Verdict::Reject);
    assert!(codes(&res_e3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_xml_ternary_arity_rejected() {
    let bad_indent2 = r#"
package main

import "encoding/xml"

fn test_it(v any, prefix string) {
    _ = xml.marshal_indent(v, prefix)
}
"#;
    let res_i2 = check(bad_indent2);
    assert_eq!(res_i2.verdict, Verdict::Reject);
    assert!(codes(&res_i2).contains(&"HYB-E1002".to_string()));

    let bad_indent4 = r#"
package main

import "encoding/xml"

fn test_it(v any, prefix string, indent string) {
    _ = xml.marshal_indent(v, prefix, indent, "extra")
}
"#;
    let res_i4 = check(bad_indent4);
    assert_eq!(res_i4.verdict, Verdict::Reject);
    assert!(codes(&res_i4).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_xml_variadic_spread_rejected() {
    let bad_spread = r#"
package main

import "encoding/xml"

fn test_it(v []any) {
    _ = xml.marshal(v...)
}
"#;
    let res = check(bad_spread);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(codes(&res).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_xml_valid_calls_and_references_accept() {
    let good = r#"
package main

import "encoding/xml"

fn run(r any, w any, v any, data []byte) {
    dec := xml.new_decoder(r)
    _ = dec
    enc := xml.new_encoder(w)
    _ = enc
    m := xml.marshal(v)
    _ = m
    mi := xml.marshal_indent(v, "", "  ")
    _ = mi
    u := xml.unmarshal(data, v)
    _ = u
    esc := xml.escape_text(w, data)
    _ = esc
    _ = xml.header
    _ = xml.html_auto_close
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
