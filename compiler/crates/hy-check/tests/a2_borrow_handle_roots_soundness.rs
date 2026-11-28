use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test must parse");
    check_file(&ast)
}

#[test]
fn a2_loop_borrow_via_composite_literal_field_rejects_mutation() {
    let src = "
import \"rust/serde_json\"
struct holder { doc serde_json.value }
fn main() (err error) {
    d := serde_json.parse(\"[1, 2]\")?
    for _ in serde_json.array((holder{doc: d}).doc) {
        serde_json.set(d, \"k\", 1)
    }
    return nil
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3101"),
        "expected HYB-E3101 on mutating `d` inside loop over borrowed handle view, got {:?}",
        r.diagnostics
    );
}

#[test]
fn a2_view_binding_from_composite_literal_or_call_field_rejects_mutation() {
    let src = "
import \"rust/serde_json\"
struct holder { doc serde_json.value }
fn id_holder(x holder) holder { return x }
fn main() (err error) {
    d := serde_json.parse(\"[]\")?
    v := serde_json.get((holder{doc: d}).doc, \"k\")
    serde_json.set(d, \"k\", 1)
    _ = serde_json.int(v)
    return nil
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3101"),
        "expected HYB-E3101 on mutating `d` while view `v` is live, got {:?}",
        r.diagnostics
    );

    let src_call = "
import \"rust/serde_json\"
struct holder { doc serde_json.value }
fn id_holder(x holder) holder { return x }
fn main() (err error) {
    d := serde_json.parse(\"[]\")?
    h := holder{doc: move d}
    v := serde_json.get(id_holder(h).doc, \"k\")
    serde_json.set(h.doc, \"k\", 1)
    _ = serde_json.int(v)
    return nil
}
";
    let r2 = check(src_call);
    assert_eq!(r2.verdict, Verdict::Reject, "expected Reject, got {:?}", r2.diagnostics);
    assert!(
        r2.diagnostics.iter().any(|d| d.code == "HYB-E3101"),
        "expected HYB-E3101 on mutating `h.doc` while call-projected view `v` is live, got {:?}",
        r2.diagnostics
    );
}
