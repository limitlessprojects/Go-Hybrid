//! wrapped_move_soundness.rs — regression tests for issue #361:
//! wrapped move expressions and may-share move rebinds bypass
//! HYB-E3003 / HYB-E3004 / HYB-E3006.

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

#[test]
fn wrapped_move_field_projection_freezes_binding_e3003() {
    let src = "\
struct box { m map[string]int }

fn worker(x map[string]int, done chan int) {
  x[\"k\"] = 1
  done <- 1
}

fn main() {
  done := make(chan int, 1)
  m := map[string]int{\"k\": 0}
  t := box{m: move m}.m
  task worker(move t, done)
  m[\"k\"] = 2
  _ = <-done
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for write to moved map after field projection wrap, got: {:?}",
        c
    );
}

#[test]
fn wrapped_move_index_projection_freezes_binding_e3003() {
    let src = "\
fn worker(x map[string]int, done chan int) {
  x[\"k\"] = 1
  done <- 1
}

fn main() {
  done := make(chan int, 1)
  m := map[string]int{\"k\": 0}
  t := []map[string]int{move m}[0]
  task worker(move t, done)
  m[\"k\"] = 2
  _ = <-done
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for write to moved map after index wrap, got: {:?}",
        c
    );
}

#[test]
fn wrapped_move_paren_slice_freezes_binding_e3003() {
    let src = "\
fn worker(x []int, done chan int) {
  x[0] = 1
  done <- 1
}

fn main() {
  done := make(chan int, 1)
  s := []int{1, 2, 3}
  t := (move s)[0:2]
  task worker(move t, done)
  s[0] = 2
  _ = <-done
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for write to moved slice after paren-slice wrap, got: {:?}",
        c
    );
}

#[test]
fn wrapped_move_call_freezes_binding_e3003() {
    let src = "\
fn same(m map[string]int) map[string]int {
  return m
}

fn worker(x map[string]int, done chan int) {
  x[\"k\"] = 1
  done <- 1
}

fn main() {
  done := make(chan int, 1)
  m := map[string]int{\"k\": 0}
  t := same(move m)
  task worker(move t, done)
  m[\"k\"] = 2
  _ = <-done
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for write to moved map after call wrap, got: {:?}",
        c
    );
}

#[test]
fn may_share_peer_rebind_move_rejects_e3004() {
    let src = "\
struct holder { m map[string]int }

fn worker(h holder, done chan int) {
  h.m[\"k\"] = 1
  done <- 1
}

fn leak(p map[string]int, done chan int) {
  p2 := holder{m: p}
  var h = holder{m: map[string]int{\"x\": 0}}
  h = move p2
  task worker(move h, done)
}

fn main() {
  done := make(chan int, 1)
  m := map[string]int{\"k\": 0}
  leak(m, done)
  m[\"k\"] = 2
  _ = <-done
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 for moving rebound holder sharing param storage, got: {:?}",
        c
    );
}

#[test]
fn wrapped_move_gc_field_rejects_e3004() {
    let src = "\
struct inner { data map[string]int }
struct holder { data map[string]int }

fn main() {
  s := inner{data: map[string]int{\"k\": 0}}
  t := holder{data: move s.data}.data
  _ = t
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 for wrapped GC field move, got: {:?}",
        c
    );
}

#[test]
fn const_composite_move_write_and_delete_rejects_e3006() {
    let src = "\
const m = map[string]int{\"k\": 1}

fn main() {
  t := move m
  t[\"k\"] = 99
  delete(move m, \"k\")
}
";
    let r = check(src);
    let c = codes(&r);
    assert!(
        c.iter().filter(|code| *code == "HYB-E3006").count() >= 2,
        "expected at least two HYB-E3006 errors for write and delete of moved const composite, got: {:?}",
        c
    );
}
