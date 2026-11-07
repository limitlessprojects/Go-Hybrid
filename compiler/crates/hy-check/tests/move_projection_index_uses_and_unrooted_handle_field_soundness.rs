//! Regression tests for two soundness holes in `Expr::Move` projection handling:
//! 1. Index and slice-bound subexpressions dropped inside rooted `move` projections (`walk::expr_uses`).
//! 2. Unrooted own-handle field moves (`op.root_ident() == None`) bypassing `HYB-E3004` and freeze.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("move_projection_index_uses_and_unrooted_handle_field_soundness.hy", src)
        .expect("test program must parse");
    check_file(&ast)
}

fn assert_reject_codes(src: &str, expected_codes: &[&str], label: &str) {
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "{label}: expected Reject, got {:?}", res.diagnostics);
    for code in expected_codes {
        assert!(
            res.diagnostics.iter().any(|d| d.code == *code),
            "{label}: expected {code}, got {:?}",
            res.diagnostics
        );
    }
}

#[test]
fn move_projection_index_and_slice_bounds_reject_use_after_move() {
    assert_reject_codes(
        r#"
import "serde_json"
struct holder { doc serde_json.value }
struct state { holders []holder }
fn sink_map(m map[string]int) { _ = m }
fn run(s state) error {
    idx_map := map[string]int{"k": 0}
    task sink_map(move idx_map)
    doc := move s.holders[idx_map["k"]].doc
    _ = doc
    return nil
}
"#,
        &["HYB-E3003"],
        "moved map read inside move projection index at local define",
    );

    assert_reject_codes(
        r#"
import "serde_json"
struct holder { doc serde_json.value }
struct state { holders []holder }
fn sink_map(m map[string]int) { _ = m }
fn sink_doc(d serde_json.value) { _ = d }
fn run(s state) {
    idx_map := map[string]int{"lo": 0, "hi": 1}
    task sink_map(move idx_map)
    task sink_doc(move s.holders[idx_map["lo"]:idx_map["hi"]][0].doc)
}
"#,
        &["HYB-E3003"],
        "moved map read inside move projection slice bounds at task spawn",
    );

    assert_reject_codes(
        r#"
import "serde_json"
struct holder { doc serde_json.value }
struct state { holders []holder }
fn sink_map(m map[string]int) { _ = m }
fn run(s state, ch chan<- serde_json.value) {
    idx_map := map[string]int{"k": 0}
    task sink_map(move idx_map)
    ch <- move s.holders[idx_map["k"]].doc
}
"#,
        &["HYB-E3003"],
        "moved map read inside move projection index at channel send",
    );
}

#[test]
fn move_projection_index_rejects_same_stmt_move_and_consume() {
    assert_reject_codes(
        r#"
import "serde_json"
struct holder { doc serde_json.value }
struct state { holders []holder }
fn sink_both(m map[string]int, d serde_json.value) {
    _ = m
    _ = d
}
fn run(s state) {
    idx_map := map[string]int{"k": 0}
    task sink_both(move idx_map, move s.holders[idx_map["k"]].doc)
}
"#,
        &["HYB-E3003"],
        "same-stmt move + read inside move projection index",
    );

    assert_reject_codes(
        r#"
import "rust/flate2"
import "serde_json"
struct holder { doc serde_json.value }
struct state { holders []holder }
fn run(s state, data []byte) error {
    enc := flate2.new_writer(6)?
    out, doc := enc.finish()?, move s.holders[enc.write(data)?].doc
    _ = out
    _ = doc
    return nil
}
"#,
        &["HYB-E3003"],
        "same-stmt consume + use inside move projection index",
    );
}

#[test]
fn move_projection_index_extends_map_slot_loan_lifetime_and_rejects_addr_of_loan() {
    assert_reject_codes(
        r#"
import "serde_json"
struct entry { idx int }
struct holder { doc serde_json.value }
struct state { holders []holder }
fn run(s state) {
    m := map[string]entry{"a": entry{idx: 0}}
    slot := &m["a"]
    m["b"] = entry{idx: 1}
    doc := move s.holders[slot.idx].doc
    _ = doc
}
"#,
        &["HYB-E3102"],
        "map mutated before final slot-loan read inside move projection index",
    );

    assert_reject_codes(
        r#"
import "serde_json"
struct entry { idx int }
struct holder { doc serde_json.value }
struct state { holders []holder }
fn read_idx(p *entry) int { return p.idx }
fn run(s state) {
    m := map[string]entry{"a": entry{idx: 0}}
    slot := &m["a"]
    doc := move s.holders[read_idx(&slot)].doc
    _ = doc
}
"#,
        &["HYB-E3104"],
        "taking &slot inside move projection index",
    );
}

#[test]
fn unrooted_own_handle_field_moves_are_rejected_and_freeze_underlying_binding() {
    assert_reject_codes(
        r#"
import "serde_json"
struct holder { doc serde_json.value }
fn worker(d serde_json.value) { _ = d }
fn run() error {
    raw := serde_json.parse("null")?
    h := holder{doc: move raw}
    task worker(move (holder{doc: h.doc}).doc)
    _ = h.doc.get("k")
    return nil
}
"#,
        &["HYB-E3004", "HYB-E3003"],
        "unrooted struct-literal own-handle field move at task spawn",
    );

    assert_reject_codes(
        r#"
import "serde_json"
struct holder { doc serde_json.value }
fn passthrough(p *holder) *holder { return p }
fn run(ch chan<- serde_json.value) error {
    raw := serde_json.parse("null")?
    h := holder{doc: move raw}
    ch <- move passthrough(&h).doc
    _ = h.doc.get("k")
    return nil
}
"#,
        &["HYB-E3004", "HYB-E3003"],
        "call-result own-handle field move at channel send",
    );

    assert_reject_codes(
        r#"
import "serde_json"
struct holder { doc serde_json.value }
fn run() error {
    raw := serde_json.parse("null")?
    h := holder{doc: move raw}
    moved_doc := move ([]holder{h}[0]).doc
    _ = moved_doc
    _ = h.doc.get("k")
    return nil
}
"#,
        &["HYB-E3004", "HYB-E3003"],
        "unrooted slice-literal own-handle field move at local define",
    );
}

#[test]
fn valid_rooted_own_handle_field_moves_still_pass() {
    let ok = check(
        r#"
import "serde_json"
struct entry { idx int }
struct holder { doc serde_json.value }
struct state { holders []holder }
fn worker(d serde_json.value) { _ = d }
fn run() error {
    raw := serde_json.parse("null")?
    s := state{holders: []holder{holder{doc: move raw}}}
    m := map[string]entry{"a": entry{idx: 0}}
    slot := &m["a"]
    doc := move s.holders[slot.idx].doc
    m["b"] = entry{idx: 1}
    task worker(move doc)
    return nil
}
"#,
    );
    assert_eq!(ok.verdict, Verdict::Accept, "expected Accept, got: {:?}", ok.diagnostics);
}
