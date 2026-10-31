use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn match_arm_self_shadow_rebind_then_move_inner_alias_rejects_outer_use() {
    let out = check(
        r#"
fn wrap(m map[string]int) option[map[string]int] { return some(m) }
fn worker(m map[string]int, done chan bool) {
  m["worker"] = 1
  done <- true
}
fn main() {
  m := map[string]int{"init": 0}
  done := make(chan bool, 1)
  match wrap(m) {
    some(m) => {
      inner := m
      m = map[string]int{}
      task worker(move inner, done)
    }
    none => {}
  }
  m["main"] = 2
  <-done
}
"#,
    );
    assert_eq!(out.verdict, Verdict::Reject, "diags: {:?}", out.diagnostics);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got: {:?}",
        out.diagnostics
    );
}

#[test]
fn multi_value_local_self_shadow_rebind_then_move_inner_alias_rejects_outer_use() {
    let out = check(
        r#"
fn pass(m map[string]int) (map[string]int, bool) { return m, true }
fn worker(m map[string]int, done chan bool) {
  m["worker"] = 1
  done <- true
}
fn main() {
  m := map[string]int{"init": 0}
  done := make(chan bool, 1)
  if true {
    m, ok := pass(m)
    if ok {
      inner := m
      m = map[string]int{}
      task worker(move inner, done)
    }
  }
  m["main"] = 2
  <-done
}
"#,
    );
    assert_eq!(out.verdict, Verdict::Reject, "diags: {:?}", out.diagnostics);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got: {:?}",
        out.diagnostics
    );
}

#[test]
fn match_and_multidef_self_shadow_preserve_prior_child_aliases_on_move() {
    let out = check(
        r#"
fn wrap(m map[string]int) option[map[string]int] { return some(m) }
fn worker(m map[string]int, done chan bool) {
  m["worker"] = 1
  done <- true
}
fn main() {
  m := map[string]int{"init": 0}
  alias := m
  done := make(chan bool, 1)
  match wrap(m) {
    some(m) => {
      task worker(move m, done)
      alias["race"] = 2
    }
    none => {}
  }
  <-done
}
"#,
    );
    assert_eq!(out.verdict, Verdict::Reject, "diags: {:?}", out.diagnostics);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got: {:?}",
        out.diagnostics
    );
}
