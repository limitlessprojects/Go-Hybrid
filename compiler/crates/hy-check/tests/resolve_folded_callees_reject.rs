//! Reject tests for `HYB-E1001` (`resolve.rs`, issue #2423):
//! 1. Local/parameter/closure bindings shadowing a `PREDECLARED` or file-level
//!    name in folded bracketed callees (`copy[undef_shadow_key](42)`).
//! 2. Folded bracketed callees on `Expr::Field` (`r.header[undef_field_key](42)`
//!    and `http.status_ok[undef_http_t]()`).
//! 3. Qualified `match` arm pattern roots (`undef_enum.variant:` and unknown
//!    enum variants `color.ghost_variant:`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let f = parse_file("test.hy", src).expect("parse ok");
    let out = check_file(&f);
    (out.verdict, out.diagnostics)
}

#[test]
fn shadowed_predeclared_and_fn_folded_callees_reject_e1001() {
    let src = r#"
fn id[t any](x t) t {
    return x
}

fn run(copy map[string]fn(int)) {
    copy[undef_param_shadow_key](42)
    min := copy
    min[undef_local_shadow_key](42)
    id := copy
    id[undef_fn_shadow_key](42)
}

fn main() {}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(verdict, Verdict::Reject);
    for expected in [
        "undef_param_shadow_key",
        "undef_local_shadow_key",
        "undef_fn_shadow_key",
    ] {
        assert!(
            diags
                .iter()
                .any(|d| d.code == "HYB-E1001" && d.message.contains(expected)),
            "expected HYB-E1001 naming `{expected}`, got: {diags:?}"
        );
    }
}

#[test]
fn folded_field_callees_and_curated_http_brackets_reject_e1001() {
    let src = r#"
import "net/http"

struct holder {
    fns map[string]fn(int)
}

fn run(h holder, r *http.request) {
    h.fns[undef_holder_key](42)
    r.header[undef_header_key](42)
    _ = http.status_ok[undef_http_t]()
    _ = http.method_get[int]()
}

fn main() {}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(verdict, Verdict::Reject);
    for expected in [
        "undef_holder_key",
        "undef_header_key",
        "undef_http_t",
        "http.method_get[int]",
    ] {
        assert!(
            diags
                .iter()
                .any(|d| d.code == "HYB-E1001" && d.message.contains(expected)),
            "expected HYB-E1001 naming `{expected}`, got: {diags:?}"
        );
    }
}

#[test]
fn qualified_match_arm_undeclared_root_and_unknown_variant_reject_e1001() {
    let (verdict, diags) = check(
        r#"
fn check_val(x int) {
    match x {
        undef_enum.variant: {}
        _: {}
    }
}

fn main() {}
"#,
    );
    assert_eq!(verdict, Verdict::Reject);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("undef_enum")),
        "expected HYB-E1001 naming `undef_enum`, got: {diags:?}"
    );

    let (_, enum_diags) = check(
        r#"
enum color {
    red
    blue
}

fn main() {
    c := red
    match c {
        color.ghost_variant: {}
        _: {}
    }
}
"#,
    );
    assert!(
        enum_diags
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("color.ghost_variant")),
        "expected HYB-E1001 naming `color.ghost_variant`, got: {enum_diags:?}"
    );
}
