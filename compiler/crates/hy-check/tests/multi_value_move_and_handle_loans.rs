//! multi_value_move_and_handle_loans.rs — regression tests for:
//! 1. Multi-value local (`a, n := f(move m)`) and multi-value assign (`a, n = f(move m)`)
//!    move-marking, non-owned origin propagation, self-shadowing freeze links,
//!    shared-root return alias linking, closure `const` write detection, and
//!    composite-literal nil checking (O5.1 / O5.2 / O5.4 / T22 / N1).
//! 2. Boundary handle `CallMode::Consume` (`doc.release()`, `serde_json.release(doc)`)
//!    and moved/rebound handle (`d2 := move doc`, `d2 = move doc`, loop-body `move doc`)
//!    borrow-loan conflict detection (A2 / O7, HYB-E3101).
//! 3. Parenthesized expression sendability classification (`(m)`, `(&x)`, `(box{m: m})`),
//!    `ch <- move v` channel-element type mismatch (`HYB-E3011`), and `OwnHandle`
//!    field-chain root validation (`move (doc_box{doc: d}).doc` vs `move (hb).doc`).

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ===========================================================================
// 1. Multi-value Local & Assign move / origin / shadow / const / nil rules
// ===========================================================================

#[test]
fn multi_value_local_move_freezes_source_and_peers() {
    let src = "\
fn split_pair(x map[string]int) (map[string]int, int) {
  return x, 1
}

fn main() {
  m := map[string]int{\"k\": 1}
  alias := m
  a, n := split_pair(move m)
  m[\"k\"] = 2
  alias[\"k\"] = 3
  _ = a
  _ = n
}
";
    let c = codes(&check(src));
    let e3003_count = c.iter().filter(|code| *code == "HYB-E3003").count();
    assert!(
        e3003_count >= 2,
        "expected HYB-E3003 on both `m` and `alias` after `a, n := split_pair(move m)`, got: {:?}",
        c
    );
}

#[test]
fn multi_value_local_gc_field_move_rejected() {
    let src = "\
struct holder { m map[string]int }

fn split_pair(x map[string]int) (map[string]int, int) {
  return x, 1
}

fn main() {
  h := holder{m: map[string]int{\"k\": 1}}
  a, n := split_pair(move h.m)
  _ = a
  _ = n
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 for `a, n := split_pair(move h.m)`, got: {:?}",
        c
    );
}

#[test]
fn multi_value_local_moved_param_propagates_non_owned_origin() {
    let src = "\
fn split_pair(x map[string]int) (map[string]int, int) {
  return x, 1
}

fn worker(m map[string]int, done chan<- bool) {
  m[\"k\"] = 2
  done <- true
}

fn bad(p map[string]int) {
  done := make(chan bool, 1)
  a, n := split_pair(move p)
  task worker(move a, done)
  fmt.println(n)
  <-done
}

fn main() {}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 when moving parameter `p` via multi-value local, got: {:?}",
        c
    );
}

#[test]
fn multi_value_assign_self_rebind_moved_param_preserves_non_owned_origin() {
    let src = "\
fn split_pair(x map[string]int) (map[string]int, int) {
  return x, 1
}

fn worker(m map[string]int, done chan<- bool) {
  m[\"k\"] = 2
  done <- true
}

fn bad(p map[string]int) {
  done := make(chan bool, 1)
  var n int = 0
  p, n = split_pair(move p)
  task worker(move p, done)
  fmt.println(n)
  <-done
}

fn main() {}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 when self-rebinding parameter `p, n = split_pair(move p)` and then moving `p`, got: {:?}",
        c
    );
}

#[test]
fn multi_value_local_inner_shadow_preserves_outer_freeze() {
    let src = "\
fn swap_int_map(x map[string]int) (int, map[string]int) {
  return 1, x
}

fn worker(x map[string]int, done chan<- bool) {
  x[\"k\"] = 2
  done <- true
}

fn main() {
  done := make(chan bool, 1)
  m := map[string]int{\"k\": 1}
  if true {
    n, m := swap_int_map(m)
    task worker(move m, done)
    fmt.println(n)
  }
  m[\"k\"] = 3
  <-done
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 on outer `m` after inner self-shadowing `n, m := swap_int_map(m)` is moved, got: {:?}",
        c
    );
}

#[test]
fn multi_value_local_self_shadow_const_write_rejected() {
    let src = "\
const const_map = map[string]int{\"k\": 1}

fn swap_int_map(x map[string]int) (int, map[string]int) {
  return 1, x
}

fn main() {
  if true {
    n, const_map := swap_int_map(const_map)
    const_map[\"k\"] = 2
    fmt.println(n)
  }
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 when writing through self-shadowing multi-value `n, const_map := swap_int_map(const_map)`, got: {:?}",
        c
    );
}

#[test]
fn multi_value_local_and_const_assign_composite_nils_rejected() {
    let src = "\
struct box { m map[string]int }
const const_box = box{m: map[string]int{\"k\": 1}}

fn split_pair(x map[string]int) (map[string]int, int) {
  return x, 1
}

fn main() {
  a, n := split_pair(box{m: nil}.m)
  const_box = box{m: nil}
  _ = a
  _ = n
}
";
    let c = codes(&check(src));
    let e4001_count = c.iter().filter(|code| *code == "HYB-E4001").count();
    assert!(
        e4001_count >= 2 && c.contains(&"HYB-E3006".to_string()),
        "expected two HYB-E4001 diagnostics and HYB-E3006, got: {:?}",
        c
    );
}

#[test]
fn closure_multi_value_moved_const_write_rejected() {
    let src = "\
const const_map = map[string]int{\"k\": 1}

fn split_pair(x map[string]int) (map[string]int, int) {
  return x, 1
}

fn main() {
  f := fn() {
    a, n := split_pair(move const_map)
    a[\"k\"] = 2
    fmt.println(n)
  }
  g := fn() {
    var b map[string]int = map[string]int{\"k\": 0}
    var n int = 0
    b, n = split_pair(move const_map)
    b[\"k\"] = 3
    fmt.println(n)
  }
  f()
  g()
}
";
    let c = codes(&check(src));
    let e3006_count = c.iter().filter(|code| *code == "HYB-E3006").count();
    assert!(
        e3006_count >= 2,
        "expected HYB-E3006 on both closure writes through moved `const_map`, got: {:?}",
        c
    );
}

// ===========================================================================
// 2. Boundary handle Consume & moved-handle A2 borrow-loan conflicts
// ===========================================================================

#[test]
fn consume_method_during_live_borrow_view_rejected() {
    let src = "\
import \"rust/flate2\"
import \"rust/serde_json\"

fn main() {
  enc := flate2.new_writer(6)?
  items := serde_json.array(enc)?
  out := enc.finish()?
  fmt.println(len(out))
  fmt.println(items.len())
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3101".to_string()),
        "expected HYB-E3101 when `enc.finish()` runs while borrow view `items` is live, got: {:?}",
        c
    );
}

#[test]
fn consume_pkg_fn_during_live_borrow_view_rejected() {
    let src = "\
import \"rust/flate2\"
import \"rust/serde_json\"

fn main() {
  enc := flate2.new_writer(6)?
  items := serde_json.array(enc)?
  out := flate2.finish(enc)?
  fmt.println(len(out))
  fmt.println(items.len())
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3101".to_string()),
        "expected HYB-E3101 when `flate2.finish(enc)` runs while borrow view `items` is live, got: {:?}",
        c
    );
}

#[test]
fn moved_and_rebound_handle_mutate_during_live_borrow_view_rejected() {
    let src = "\
import \"rust/serde_json\"

fn bad_define() {
  doc := serde_json.parse(`{}`)?
  items := doc.array(\"items\")?
  d2 := move doc
  d2.set(\"k\", 1)?
  fmt.println(items.len())
}

fn bad_rebind() {
  doc := serde_json.parse(`{}`)?
  items := doc.array(\"items\")?
  var d2 serde_json.value = serde_json.parse(`{}`)?
  d2 = move doc
  d2.set(\"k\", 1)?
  fmt.println(items.len())
}

fn main() {}
";
    let c = codes(&check(src));
    let e3101_count = c.iter().filter(|code| *code == "HYB-E3101").count();
    assert!(
        e3101_count >= 2,
        "expected HYB-E3101 on both `d2 := move doc; d2.set(...)` and `d2 = move doc; d2.set(...)` while `items` is live, got: {:?}",
        c
    );
}

#[test]
fn move_handle_inside_loop_over_borrow_view_rejected() {
    let src = "\
import \"rust/serde_json\"

fn ship(d serde_json.value, done chan<- bool) {
  done <- true
}

fn main() {
  done := make(chan bool, 1)
  doc := serde_json.parse(`{}`)?
  for u in doc.array(\"users\")? {
    fmt.println(u.int(\"id\")?)
    task ship(move doc, done)
    break
  }
  <-done
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3101".to_string()),
        "expected HYB-E3101 when moving `doc` inside a loop iterating `doc.array(...)`, got: {:?}",
        c
    );
}

#[test]
fn consume_after_last_use_of_borrow_view_accepted() {
    let src = "\
import \"rust/flate2\"
import \"rust/serde_json\"

fn main() {
  enc := flate2.new_writer(6)?
  items := serde_json.array(enc)?
  fmt.println(items.len())
  out := enc.finish()?
  fmt.println(len(out))
}
";
    let c = codes(&check(src));
    assert!(
        c.is_empty(),
        "expected 0 errors when `enc.finish()` runs after the last use of `items`, got: {:?}",
        c
    );
}

// ===========================================================================
// 3. Parenthesized sendability, moved channel mismatch, OwnHandle field root
// ===========================================================================

#[test]
fn parenthesized_non_sendable_values_rejected_at_spawn_and_send() {
    let src = "\
struct box { m map[string]int }

fn worker_map(m map[string]int, done chan<- bool) {
  done <- true
}

fn worker_ptr(p *int, done chan<- bool) {
  done <- true
}

fn worker_box(b box, done chan<- bool) {
  done <- true
}

fn main() {
  done := make(chan bool, 1)
  ch := make(chan map[string]int, 1)
  m := map[string]int{\"k\": 1}
  var x int = 1
  task worker_map((m), done)
  ch <- (m)
  task worker_ptr((&x), done)
  task worker_box((box{m: m}), done)
}
";
    let c = codes(&check(src));
    let e3001_count = c.iter().filter(|code| *code == "HYB-E3001").count();
    let e3002_count = c.iter().filter(|code| *code == "HYB-E3002").count();
    assert!(
        e3001_count >= 3 && e3002_count >= 1,
        "expected >=3 HYB-E3001 and >=1 HYB-E3002 for parenthesized non-sendable args, got: {:?}",
        c
    );
}

#[test]
fn move_send_channel_elem_mismatch_rejected() {
    let src = "\
fn main() {
  ch := make(chan int, 1)
  m1 := map[string]int{\"k\": 1}
  m2 := map[string]int{\"k\": 2}
  ch <- move m1
  select {
    ch <- move m2: {}
    default: {}
  }
}
";
    let c = codes(&check(src));
    let e3011_count = c.iter().filter(|code| *code == "HYB-E3011").count();
    assert!(
        e3011_count >= 2,
        "expected HYB-E3011 on both `ch <- move m1` and `select {{ ch <- move m2 }}`, got: {:?}",
        c
    );
}

#[test]
fn own_handle_field_move_from_temporary_rejected_while_paren_ident_freezes() {
    let src = "\
import \"rust/serde_json\"

struct doc_box { doc serde_json.value }

fn bad_temp_field_move() {
  d := serde_json.parse(`{}`)?
  d2 := move (doc_box{doc: d}).doc
  _ = d2
}

fn bad_paren_base_use_after_move() {
  hb := doc_box{doc: serde_json.parse(`{}`)?}
  d3 := move (hb).doc
  _ = d3
  hb.doc.set(\"k\", 1)?
}

fn main() {}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3004".to_string()) && c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3004 for `move (doc_box{{doc: d}}).doc` and HYB-E3003 for use after `move (hb).doc`, got: {:?}",
        c
    );
}
