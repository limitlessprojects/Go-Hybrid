//! Regression tests for issue #1612: multi-value destructuring defines inside
//! closures must record borrow aliases (O2 `HYB-E3104` + O5.2 `HYB-E3003`), and
//! closure A3 map-slot loan liveness must account for implicit bare `return`
//! reads of named result bindings (`HYB-E3102`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("closure_multi_borrow_and_named_result_loans.hy", src).expect("parses");
    check_file(&ast)
}

#[test]
fn closure_multi_value_define_rejects_borrow_return_escape_e3104() {
    let r = check(
        r#"fn pair_ptr(p *int) (*int, bool) {
    return p, true
}

fn main() {
    f := fn() *int {
        x := 1
        p, ok := pair_ptr(&x)
        _ = ok
        return p
    }
    _ = f()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 on returned multi-value borrow inside closure: {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_multi_value_define_links_borrow_alias_for_o52_freeze() {
    let r = check(
        r#"struct bag {
    items map[string]int
}

fn pair_bag(p *bag) (*bag, bool) {
    return p, true
}

fn worker(b bag) {
    b.items["k"] = 2
}

fn main() {
    run := fn() {
        b := bag{items: map[string]int{"k": 1}}
        p, ok := pair_bag(&b)
        _ = ok
        task worker(move b)
        _ = p.items["k"]
    }
    run()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when reading multi-value borrow alias after move: {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_named_result_slot_loan_extends_across_bare_return_e3102() {
    let r = check(
        r#"type int_ptr = *int

fn main() {
    f := fn() (out int_ptr) {
        m := map[int]int{1: 10}
        out = &m[1]
        m[2] = 20
        return
    }
    _ = f()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 on map insert while named-result slot loan is live to bare return: {:?}",
        r.diagnostics
    );
}
