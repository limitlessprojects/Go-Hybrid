//! Regression tests for issue #1611: SPEC O5.4 own-handle field moves must be
//! rooted at a struct binding chain (`s.doc`, `w.s.doc`), never a composite
//! literal, call rvalue, or indexed GC container (`(wrap{h: h}).h.doc`,
//! `id(h).doc`, `xs[0].doc`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("own_handle_non_place_field_move.hy", src).expect("parses");
    check_file(&ast)
}

#[test]
fn composite_literal_own_handle_field_move_is_rejected_with_e3004() {
    let r = check(
        r#"import "rust/serde_json"

struct holder {
    doc serde_json.value
}

struct wrap {
    h holder
}

fn ship(d serde_json.value) {
    _ = d
}

fn main() {
    h := holder{doc: serde_json.parse("[]")?}
    d1 := move (wrap{h: h}).h.doc
    d2 := move h.doc
    task ship(move d1)
    task ship(move d2)
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 on non-place OwnHandle field move: {:?}",
        r.diagnostics
    );
}

#[test]
fn call_and_slice_index_own_handle_field_moves_are_rejected() {
    let r_call = check(
        r#"import "rust/serde_json"

struct holder {
    doc serde_json.value
}

fn id(h holder) holder {
    return h
}

fn ship(d serde_json.value) {
    _ = d
}

fn main() {
    h := holder{doc: serde_json.parse("[]")?}
    d := move id(h).doc
    task ship(move d)
}
"#,
    );
    assert_eq!(r_call.verdict, Verdict::Reject, "{:?}", r_call.diagnostics);
    assert!(
        r_call.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 on call-result OwnHandle field move: {:?}",
        r_call.diagnostics
    );

    let r_index = check(
        r#"import "rust/serde_json"

struct holder {
    doc serde_json.value
}

fn main() {
    xs := []holder{holder{doc: serde_json.parse("[]")?}}
    d := move xs[0].doc
    _ = d
}
"#,
    );
    assert_eq!(
        r_index.verdict,
        Verdict::Reject,
        "{:?}",
        r_index.diagnostics
    );
    assert!(
        r_index.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 on indexed slice OwnHandle field move: {:?}",
        r_index.diagnostics
    );
}

#[test]
fn binding_rooted_nested_own_handle_field_move_remains_accepted_and_freezes_root() {
    let ok = check(
        r#"import "rust/serde_json"

struct holder {
    doc serde_json.value
}

struct wrap {
    h holder
}

fn ship(d serde_json.value) {
    _ = d
}

fn main() {
    w := wrap{h: holder{doc: serde_json.parse("[]")?}}
    d := move (w.h).doc
    task ship(move d)
}
"#,
    );
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);

    let use_after = check(
        r#"import "rust/serde_json"

struct holder {
    doc serde_json.value
}

struct wrap {
    h holder
}

fn main() {
    w := wrap{h: holder{doc: serde_json.parse("[]")?}}
    d := move (w.h).doc
    _ = d
    w.h.doc.set("k", 1)?
}
"#,
    );
    assert_eq!(
        use_after.verdict,
        Verdict::Reject,
        "{:?}",
        use_after.diagnostics
    );
    assert!(
        use_after.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 after binding-rooted OwnHandle field move: {:?}",
        use_after.diagnostics
    );
}
