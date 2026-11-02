//! foreign_field_2203.rs - a field selection on a foreign handle that its
//! binding does not ship (issue #2203, Row 13 BUILDABLE).
//!
//! The method half is #2200 (fixed by foreign_method_surface at the
//! method-call site). Field selection was never checked, so
//! doc.totally_fake_field passed hyb check with 0 diagnostics and died at
//! go build with `doc.TotallyFakeField undefined`.
//!
//! Only handles whose Go binding is vendored in runtime/go/ are modelled;
//! everything else keeps the old fail-open behaviour.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e1002(r: &hy_check::CheckResult, what: &str) {
    let cs = codes(r);
    assert!(cs.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002 ({}), got {:?}", what, cs);
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(r: &hy_check::CheckResult, what: &str) {
    let cs = codes(r);
    assert!(cs.is_empty(), "expected accept ({}), got {:?}", what, cs);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn rejects_unknown_field_read_on_serde_value() {
    let r = check("import \"rust/serde_json\"\n\nfn main() {\n    doc := serde_json.parse(\"null\")?\n    fmt.println(doc.totally_fake_field)\n}\n");
    assert_e1002(&r, "doc.totally_fake_field");
}

#[test]
fn rejects_unknown_field_inside_interpolation() {
    let r = check("import \"rust/serde_json\"\n\nfn main() {\n    doc := serde_json.parse(\"null\")?\n    fmt.println(\"v={doc.nope}\")\n}\n");
    assert_e1002(&r, "interpolated doc.nope");
}

#[test]
fn accepts_known_method_on_serde_value() {
    let r = check("import \"rust/serde_json\"\n\nfn main() {\n    doc := serde_json.parse(\"null\")?\n    fmt.println(doc.string(\"k\"))\n}\n");
    assert_accept(&r, "doc.string(k)");
}

#[test]
fn accepts_struct_field_unchanged() {
    let r = check("struct box {\n    n int\n}\n\nfn main() {\n    b := box{n: 1}\n    fmt.println(b.n)\n}\n");
    assert_accept(&r, "user struct field b.n");
}

#[test]
fn accepts_field_on_unmodelled_foreign_receiver() {
    let r = check("import \"net/http\"\n\nfn f(r *http.request) string {\n    return r.url.path\n}\n\nfn main() {\n    fmt.println(\"ok\")\n}\n");
    assert_accept(&r, "url.path on http.request");
}
