//! Regression tests for issue #4006:
//! 1. Collecting subexpression uses along rooted move projection chains
//!    (HYB-E3003, HYB-E3102, HYB-E3104).
//! 2. Enforcing HYB-E3004 on unrooted own-handle field moves.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn has_code(src: &str, code: &str) -> (Verdict, bool) {
    let ast = parse_file("inline.hy", src).expect("must parse");
    let r = check_file(&ast);
    let hit = r
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == code);
    (r.verdict, hit)
}

#[test]
fn rejects_use_after_move_inside_move_projection_index() {
    let src = r#"
import "serde_json"

struct holder {
    doc serde_json.value
}
struct state {
    holders []holder
}

fn sink(m map[string]int) {
    _ = m
}

fn run(s state) {
    idx_map := map[string]int{"k": 0}
    task sink(move idx_map)
    doc := move s.holders[idx_map["k"]].doc
    _ = doc
}

fn main() {}
"#;
    let (v, hit) = has_code(src, "HYB-E3003");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3003 for use-after-move in move projection index");
}

#[test]
fn rejects_same_stmt_move_and_use_in_move_projection_index() {
    let src = r#"
import "serde_json"

struct holder {
    doc serde_json.value
}
struct state {
    holders []holder
}

fn sink_both(m map[string]int, d serde_json.value) {
    _ = m
    _ = d
}

fn run(s state) {
    idx_map := map[string]int{"k": 0}
    task sink_both(move idx_map, move s.holders[idx_map["k"]].doc)
}

fn main() {}
"#;
    let (v, hit) = has_code(src, "HYB-E3003");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3003 for same-stmt move+use in move projection index");
}

#[test]
fn rejects_slot_loan_conflict_in_move_projection_index() {
    let src = r#"
import "serde_json"

struct entry {
    idx int
}
struct holder {
    doc serde_json.value
}
struct state {
    holders []holder
}

fn run(s state) {
    m := map[string]entry{"a": entry{idx: 0}}
    slot := &m["a"]
    m["b"] = entry{idx: 1}
    doc := move s.holders[slot.idx].doc
    _ = doc
}

fn main() {}
"#;
    let (v, hit) = has_code(src, "HYB-E3102");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3102 for conflicting map mutation before move projection index");
}

#[test]
fn rejects_unrooted_own_handle_field_move_across_task_spawn() {
    let src = r#"
import "serde_json"

struct holder {
    doc serde_json.value
}

fn worker(d serde_json.value) {
    _ = d
}

fn run() error {
    raw := serde_json.parse("null")?
    h := holder{doc: move raw}
    task worker(move (holder{doc: h.doc}).doc)
    _ = h.doc.get("k")
    return nil
}

fn main() {}
"#;
    let (v, hit) = has_code(src, "HYB-E3004");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3004 for unrooted own-handle field move in spawn");
}

#[test]
fn rejects_unrooted_own_handle_field_move_across_channel_send() {
    let src = r#"
import "serde_json"

struct holder {
    doc serde_json.value
}

fn run(ch chan serde_json.value) error {
    raw := serde_json.parse("null")?
    h := holder{doc: move raw}
    ch <- move (holder{doc: h.doc}).doc
    _ = h.doc.get("k")
    return nil
}

fn main() {}
"#;
    let (v, hit) = has_code(src, "HYB-E3004");
    assert!(v == Verdict::Reject && hit, "expected HYB-E3004 for unrooted own-handle field move in channel send");
}

#[test]
fn accepts_valid_rooted_own_handle_field_moves() {
    let src = r#"
import "serde_json"

struct holder {
    doc serde_json.value
}

fn worker(d serde_json.value) {
    _ = d
}

fn run() error {
    raw := serde_json.parse("null")?
    h := holder{doc: move raw}
    task worker(move h.doc)
    return nil
}

fn main() {}
"#;
    let ast = parse_file("inline.hy", src).expect("must parse");
    let r = check_file(&ast);
    assert_eq!(r.verdict, Verdict::Accept, "expected Verdict::Accept, got: {:?}", r.diagnostics);
}
