//! Accept control tests for `HYB-E1001` (`resolve.rs`, issue #2423):
//! Declared index keys on shadowed bindings, declared foreign field-map index
//! keys, valid top-level generic instantiations, curated `http.*`/`io.*`
//! exports, and declared qualified `match` arm patterns must not emit `HYB-E1001`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let f = parse_file("test.hy", src).expect("parse ok");
    let out = check_file(&f);
    (out.verdict, out.diagnostics)
}

#[test]
fn shadowed_bindings_field_callees_generics_and_curated_match_accept() {
    let src = r#"
import "io"
import "net/http"
import "sync"

fn id[t any](x t) t {
    return x
}

fn run(copy map[string]fn(int), r *http.request, k string, err error) int {
    copy[k](42)
    r.header[k](42)
    r.header["lit"](42)
    lazy := sync.once_value[int](fn() int {
        return http.status_ok
    })
    match err {
        io.eof: {
            return 0
        }
        _: {
            return id[int](lazy())
        }
    }
}

fn main() {}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected Verdict::Accept on valid controls, got: {diags:?}"
    );
    assert!(diags.is_empty(), "expected 0 diagnostics, got: {diags:?}");
}

#[test]
fn declared_qualified_enum_variants_do_not_emit_e1001() {
    let src = r#"
enum color {
    red
    blue
}

fn main() {
    c := red
    match c {
        color.red: {}
        color.blue: {}
    }
}
"#;
    let (_, diags) = check(src);
    assert!(
        !diags.iter().any(|d| d.code == "HYB-E1001"),
        "expected no HYB-E1001 on declared qualified enum variants, got: {diags:?}"
    );
}
