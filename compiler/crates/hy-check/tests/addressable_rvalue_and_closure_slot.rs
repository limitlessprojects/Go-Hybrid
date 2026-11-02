//! Regression tests for HYB-E2010 on composite-literal / channel-receive rvalue
//! slot loans and closure-local / closure-parameter map/string sub-stores.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn has_e2010(src: &str) -> (Verdict, bool) {
    let ast = parse_file("inline.hy", src).expect("must parse");
    let r = check_file(&ast);
    let hit = r
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == "HYB-E2010");
    (r.verdict, hit)
}

#[test]
fn rejects_composite_lit_and_receive_rvalue_projection_loans() {
    for src in [
        "struct pt { x int }\nfn main() { n := &pt{x: 1}.x\n _ = n }\n",
        "fn main() { n := &[2]int{1, 2}[0]\n _ = n }\n",
        "struct pt { x int }\nfn f(ch chan pt) { n := &(<-ch).x\n _ = n }\nfn main() {}\n",
    ] {
        let (verdict, hit) = has_e2010(src);
        assert!(verdict == Verdict::Reject && hit, "expected HYB-E2010 for:\n{src}");
    }
}

#[test]
fn rejects_closure_local_and_param_map_or_string_sub_stores() {
    for src in [
        "struct pt { x int }\nfn main() {\n f := fn() {\n m := map[string]pt{\"a\": pt{x: 1}}\n m[\"a\"].x = 2\n }\n f()\n}\n",
        "struct pt { x int }\nfn main() {\n f := fn(m map[string]pt) {\n n := &m[\"a\"].x\n _ = n\n }\n f(map[string]pt{\"a\": pt{x: 1}})\n}\n",
        "fn main() {\n f := fn(s string) {\n n := &s[0]\n _ = n\n }\n f(\"hi\")\n}\n",
    ] {
        let (verdict, hit) = has_e2010(src);
        assert!(verdict == Verdict::Reject && hit, "expected HYB-E2010 for:\n{src}");
    }
    let (v, h) = has_e2010(
        "struct pt { x int }\nfn main() {\n m := map[string]pt{\"a\": pt{x: 1}}\n f := fn(m []pt) { m[0].x = 2 }\n f([]pt{pt{x: 0}})\n _ = m\n}\n",
    );
    assert!(v == Verdict::Accept && !h);
}
