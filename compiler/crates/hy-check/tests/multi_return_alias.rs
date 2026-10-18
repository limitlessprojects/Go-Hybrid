//! multi_return_alias.rs — regression tests for multi-value returns that
//! alias each other.
//!
//! `return m, m` hands the caller two bindings over the same storage. Before
//! this fix each result was bound (and frozen) per name, so
//! `task worker(move a, done)` left `b` live on the same map: `hyb check`
//! reported 0 diagnostics and the emitted Go raced (`go build -race`).

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
fn same_map_returned_twice_is_frozen_by_move() {
    let src = "\
fn pair() (map[string]int, map[string]int) {
  m := map[string]int{\"k\": 0}
  return m, m
}

fn worker(x map[string]int, done chan<- bool) {
  x[\"k\"] = 1
  done <- true
}

fn main() {
  done := make(chan bool, 1)
  a, b := pair()
  task worker(move a, done)
  b[\"k\"] = 2
  <-done
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for a write to the second result of `return m, m`, got: {:?}",
        c
    );
}

#[test]
fn same_struct_with_map_returned_twice_is_frozen_by_move() {
    let src = "\
struct box { m map[string]int }

fn pair() (box, box) {
  b := box{m: map[string]int{\"x\": 0}}
  return b, b
}

fn worker(x box, done chan<- bool) {
  x.m[\"k\"] = 1
  done <- true
}

fn main() {
  done := make(chan bool, 1)
  a, c := pair()
  task worker(move a, done)
  c.m[\"x\"] = 2
  <-done
}
";
    let c = codes(&check(src));
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for a struct-with-map returned twice, got: {:?}",
        c
    );
}

#[test]
fn distinct_maps_returned_together_stay_independent() {
    let src = "\
fn pair() (map[string]int, map[string]int) {
  a := map[string]int{\"x\": 0}
  b := map[string]int{\"y\": 0}
  return a, b
}

fn worker(x map[string]int, done chan<- bool) {
  x[\"k\"] = 1
  done <- true
}

fn main() {
  done := make(chan bool, 1)
  a, b := pair()
  task worker(move a, done)
  b[\"y\"] = 2
  <-done
}
";
    let r = check(src);
    assert!(
        codes(&r).is_empty(),
        "distinct return roots must not be joined, got: {:?}",
        codes(&r)
    );
}

#[test]
fn scalar_pair_is_unaffected() {
    let src = "\
fn minmax(a int, b int) (int, int) {
  if a < b { return a, b }
  return b, a
}

fn main() {
  lo, hi := minmax(3, 9)
  fmt.println(\"{lo} {hi}\")
}
";
    let r = check(src);
    assert!(
        codes(&r).is_empty(),
        "scalar multi-value returns must stay accepted, got: {:?}",
        codes(&r)
    );
}
