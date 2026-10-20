//! Regression tests for issue #455:
//! Helper-frame out-parameter and receiver stores (`dst[0] = v`, `dst["k"] = v`,
//! `h.m = v`, `copy(dst, src)`, user-defined methods, transitive helper chains,
//! and closure-body helper calls) establish a caller-visible `may_share` edge
//! between the destination and source arguments so moving either freezes the
//! other (`HYB-E3003`, O5.2) and propagates non-owned / `const` origins
//! (`HYB-E3004` O5.1/O5.4, `HYB-E3006` T22).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file)
}

#[test]
fn helper_slice_index_store_then_move_dst_freezes_src_issue_455() {
    let res = check(
        r#"
fn set(dst []map[string]int, v map[string]int) {
  dst[0] = v
}

fn worker_slice(s []map[string]int) {
  s[0]["worker"] = 1
}

fn main() {
  shared := map[string]int{"init": 0}
  dst := make([]map[string]int, 1)
  set(dst, shared)
  task worker_slice(move dst)
  shared["main"] = 2
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject, "expected Reject, got: {:?}", res.diagnostics);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on post-move write to `shared`, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn helper_slice_index_store_then_move_src_freezes_dst_issue_455() {
    let res = check(
        r#"
fn set(dst []map[string]int, v map[string]int) {
  dst[0] = v
}

fn worker_map(m map[string]int) {
  m["worker"] = 1
}

fn main() {
  shared := map[string]int{"init": 0}
  dst := make([]map[string]int, 1)
  set(dst, shared)
  task worker_map(move shared)
  dst[0]["main"] = 2
}
"#,
    );
    assert_eq!(res.verdict, Verdict::Reject, "expected Reject, got: {:?}", res.diagnostics);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on post-move write through `dst`, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn helper_map_index_store_and_pointer_field_store_freeze_peers_issue_455() {
    let res_map = check(
        r#"
fn put(dst map[string]map[string]int, v map[string]int) {
  dst["k"] = v
}

fn worker_nested(m map[string]map[string]int) {
  m["k"]["worker"] = 1
}

fn main() {
  shared := map[string]int{"init": 0}
  dst := map[string]map[string]int{}
  put(dst, shared)
  task worker_nested(move dst)
  shared["main"] = 2
}
"#,
    );
    assert_eq!(res_map.verdict, Verdict::Reject, "expected Reject, got: {:?}", res_map.diagnostics);
    assert!(
        res_map.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for map out-param store, got: {:?}",
        res_map.diagnostics
    );

    let res_ptr = check(
        r#"
struct holder {
  m map[string]int
}

fn set_field(h *holder, v map[string]int) {
  h.m = v
}

fn worker_holder(h holder) {
  h.m["worker"] = 1
}

fn main() {
  shared := map[string]int{"init": 0}
  h := holder{m: map[string]int{}}
  set_field(&h, shared)
  task worker_holder(move h)
  shared["main"] = 2
}
"#,
    );
    assert_eq!(res_ptr.verdict, Verdict::Reject, "expected Reject, got: {:?}", res_ptr.diagnostics);
    assert!(
        res_ptr.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for pointer-field out-param store, got: {:?}",
        res_ptr.diagnostics
    );
}

#[test]
fn method_receiver_and_out_param_stores_freeze_peers_issue_455() {
    let res_recv = check(
        r#"
struct bucket {
  items []map[string]int
}

fn (b bucket) store(v map[string]int) {
  b.items[0] = v
}

fn worker_bucket(b bucket) {
  b.items[0]["worker"] = 1
}

fn main() {
  shared := map[string]int{"init": 0}
  b := bucket{items: make([]map[string]int, 1)}
  b.store(shared)
  task worker_bucket(move b)
  shared["main"] = 2
}
"#,
    );
    assert_eq!(res_recv.verdict, Verdict::Reject, "expected Reject, got: {:?}", res_recv.diagnostics);
    assert!(
        res_recv.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for method storing param into receiver slice field, got: {:?}",
        res_recv.diagnostics
    );

    let res_export = check(
        r#"
struct holder {
  m map[string]int
}

fn (h holder) export_into(dst []map[string]int) {
  dst[0] = h.m
}

fn worker_slice(s []map[string]int) {
  s[0]["worker"] = 1
}

fn main() {
  h := holder{m: map[string]int{"init": 0}}
  dst := make([]map[string]int, 1)
  h.export_into(dst)
  task worker_slice(move dst)
  h.m["main"] = 2
}
"#,
    );
    assert_eq!(
        res_export.verdict,
        Verdict::Reject,
        "expected Reject, got: {:?}",
        res_export.diagnostics
    );
    assert!(
        res_export.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for method storing receiver field into out-param, got: {:?}",
        res_export.diagnostics
    );
}

#[test]
fn transitive_helper_copy_and_closure_body_stores_freeze_peers_issue_455() {
    let res_transitive = check(
        r#"
fn inner_set(dst []map[string]int, v map[string]int) {
  tmp := dst
  val := v
  tmp[0] = val
}

fn outer_set(dst []map[string]int, v map[string]int) {
  inner_set(dst, v)
}

fn worker_slice(s []map[string]int) {
  s[0]["worker"] = 1
}

fn main() {
  shared := map[string]int{"init": 0}
  dst := make([]map[string]int, 1)
  outer_set(dst, shared)
  task worker_slice(move dst)
  shared["main"] = 2
}
"#,
    );
    assert_eq!(
        res_transitive.verdict,
        Verdict::Reject,
        "expected Reject, got: {:?}",
        res_transitive.diagnostics
    );
    assert!(
        res_transitive.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for transitive helper store, got: {:?}",
        res_transitive.diagnostics
    );

    let res_copy = check(
        r#"
fn copy_maps(dst []map[string]int, src []map[string]int) {
  copy(dst, src)
}

fn worker_slice(s []map[string]int) {
  s[0]["worker"] = 1
}

fn main() {
  src := []map[string]int{map[string]int{"init": 0}}
  dst := make([]map[string]int, 1)
  copy_maps(dst, src)
  task worker_slice(move dst)
  src[0]["main"] = 2
}
"#,
    );
    assert_eq!(res_copy.verdict, Verdict::Reject, "expected Reject, got: {:?}", res_copy.diagnostics);
    assert!(
        res_copy.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for helper copy(dst, src) on reference elements, got: {:?}",
        res_copy.diagnostics
    );

    let res_closure = check(
        r#"
fn set(dst []map[string]int, v map[string]int) {
  dst[0] = v
}

fn worker_slice(s []map[string]int) {
  s[0]["worker"] = 1
}

fn main() {
  run := fn() {
    shared := map[string]int{"init": 0}
    dst := make([]map[string]int, 1)
    set(dst, shared)
    task worker_slice(move dst)
    shared["main"] = 2
  }
  run()
}
"#,
    );
    assert_eq!(
        res_closure.verdict,
        Verdict::Reject,
        "expected Reject, got: {:?}",
        res_closure.diagnostics
    );
    assert!(
        res_closure.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 inside closure body after helper store, got: {:?}",
        res_closure.diagnostics
    );
}

#[test]
fn helper_store_propagates_non_owned_and_const_origins_issue_455() {
    let res_param = check(
        r#"
fn set(dst []map[string]int, v map[string]int) {
  dst[0] = v
}

fn worker_slice(s []map[string]int) {
  s[0]["worker"] = 1
}

fn relay(p map[string]int) {
  dst := make([]map[string]int, 1)
  set(dst, p)
  task worker_slice(move dst)
}

fn main() {
  m := map[string]int{"init": 0}
  relay(m)
}
"#,
    );
    assert_eq!(
        res_param.verdict,
        Verdict::Reject,
        "expected Reject, got: {:?}",
        res_param.diagnostics
    );
    assert!(
        res_param.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving local slice holding non-owned parameter via helper store, got: {:?}",
        res_param.diagnostics
    );

    let res_const = check(
        r#"
const m = map[string]int{"init": 0}

fn set(dst []map[string]int, v map[string]int) {
  dst[0] = v
}

fn main() {
  dst := make([]map[string]int, 1)
  set(dst, m)
  dst[0]["mutate"] = 1
}
"#,
    );
    assert_eq!(
        res_const.verdict,
        Verdict::Reject,
        "expected Reject, got: {:?}",
        res_const.diagnostics
    );
    assert!(
        res_const.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when mutating const map through out-param slice alias, got: {:?}",
        res_const.diagnostics
    );
}

#[test]
fn positive_controls_without_post_move_use_or_primitive_copies_accept_issue_455() {
    let res_clean_move = check(
        r#"
fn set(dst []map[string]int, v map[string]int) {
  dst[0] = v
}

fn worker_slice(s []map[string]int) {
  s[0]["worker"] = 1
}

fn main() {
  shared := map[string]int{"init": 0}
  dst := make([]map[string]int, 1)
  set(dst, shared)
  task worker_slice(move dst)
}
"#,
    );
    assert_eq!(
        res_clean_move.verdict,
        Verdict::Accept,
        "expected Accept when neither `dst` nor `shared` is used after `move dst`, got: {:?}",
        res_clean_move.diagnostics
    );

    let res_prim_copy = check(
        r#"
fn copy_entry(dst map[string]int, src map[string]int) {
  dst["k"] = src["k"]
}

fn copy_ints(dst []int, src []int) {
  copy(dst, src)
}

fn worker_map(m map[string]int) {
  m["worker"] = 1
}

fn worker_ints(xs []int) {
  xs[0] = 9
}

fn main() {
  src := map[string]int{"k": 42}
  dst := map[string]int{"k": 0}
  copy_entry(dst, src)
  task worker_map(move dst)
  src["k"] = 7

  s_src := []int{1, 2}
  s_dst := make([]int, 2)
  copy_ints(s_dst, s_src)
  task worker_ints(move s_dst)
  s_src[0] = 99
}
"#,
    );
    assert_eq!(
        res_prim_copy.verdict,
        Verdict::Accept,
        "expected Accept for primitive value copies across helper calls, got: {:?}",
        res_prim_copy.diagnostics
    );
}
