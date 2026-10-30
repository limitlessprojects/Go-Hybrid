use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test must parse");
    check_file(&ast)
}

#[test]
fn ownhandle_field_move_on_composite_literal_freezes_underlying_binding() {
    let src = "
import \"rust/serde_json\"
struct holder { doc serde_json.value }
fn worker(v serde_json.value) {}
fn main() (err error) {
    d := serde_json.parse(\"[]\")?
    task worker(move (holder{doc: d}).doc)
    task worker(move (holder{doc: d}).doc)
    return nil
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 use-after-move, got {:?}",
        r.diagnostics
    );
}

#[test]
fn ownhandle_field_move_on_call_result_rejects_non_owned_param_and_freezes() {
    let src = "
import \"rust/serde_json\"
struct holder { doc serde_json.value }
fn id_holder(x holder) holder { return x }
fn worker(v serde_json.value) {}
fn bad(h holder) {
    task worker(move id_holder(h).doc)
    task worker(move id_holder(h).doc)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004" || d.code == "HYB-E3003"),
        "expected HYB-E3004 / HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn ownhandle_field_move_traverses_index_subexpressions_for_use_after_move() {
    let src_local = "
import \"rust/serde_json\"
struct item { doc serde_json.value }
struct state { items []item }
fn take_slice(xs []int) {}
fn take_doc(v serde_json.value) {}
fn idx_from(xs []int) int { return len(xs) }
fn bad_index() (err error) {
    d := serde_json.parse(\"[]\")?
    s := state{items: []item{item{doc: move d}}}
    xs := []int{0}
    task take_slice(move xs)
    d2 := move s.items[idx_from(xs)].doc
    task take_doc(move d2)
    return nil
}
";
    let r1 = check(src_local);
    assert_eq!(r1.verdict, Verdict::Reject, "expected Reject, got {:?}", r1.diagnostics);
    assert!(
        r1.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on moved index arg `xs`, got {:?}",
        r1.diagnostics
    );

    let src_spawn = "
import \"rust/serde_json\"
struct item { doc serde_json.value }
struct state { items map[string]item }
fn take_map(m map[string]string) {}
fn take_doc(v serde_json.value) {}
fn main() {
    s := state{items: map[string]item{}}
    m := map[string]string{\"k\": \"a\"}
    task take_map(move m)
    task take_doc(move s.items[m[\"k\"]].doc)
}
";
    let r2 = check(src_spawn);
    assert_eq!(r2.verdict, Verdict::Reject, "expected Reject, got {:?}", r2.diagnostics);
    assert!(
        r2.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on moved map `m` inside field-move index, got {:?}",
        r2.diagnostics
    );
}
