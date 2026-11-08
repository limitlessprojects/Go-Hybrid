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

#[test]
fn rejects_inferred_ptr_array_hops_and_addrof_unaddressable_places() {
    for src in [
        "struct holder { n int }\nfn main() {\n pm := &[2]map[string]holder{map[string]holder{\"a\": holder{n: 1}}, map[string]holder{}}\n pm[0][\"a\"].n = 2\n}\n",
        "struct holder { n int }\nfn main() {\n pm := &[2]map[string]holder{map[string]holder{\"a\": holder{n: 1}}, map[string]holder{}}\n n := &pm[0][\"a\"].n\n _ = n\n}\n",
        "fn main() {\n ps := &[2]string{\"ab\", \"cd\"}\n ps[0][0] = 65\n}\n",
        "fn main() {\n ps := &[2]string{\"ab\", \"cd\"}\n b := &ps[0][0]\n _ = b\n}\n",
        "fn main() {\n s := \"hi\"\n var p *byte\n &s[0] = p\n}\n",
    ] {
        let (verdict, hit) = has_e2010(src);
        assert!(verdict == Verdict::Reject && hit, "expected HYB-E2010 for:\n{src}");
    }
    let (v, h) = has_e2010(
        "struct holder { n int }\nfn main() {\n pm := &[2]map[string]holder{map[string]holder{\"a\": holder{n: 1}}, map[string]holder{}}\n pm[0][\"a\"] = holder{n: 3}\n h := &pm[0][\"a\"]\n h.n = 4\n}\n",
    );
    assert!(v == Verdict::Accept && !h);
}

