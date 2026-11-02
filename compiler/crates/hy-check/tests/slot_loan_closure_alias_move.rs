//! #2624: O8 map/slice slot loans (`s := &m[k]`) must reject `move s` inside nested closures,
//! local pointer alias copies (`p := slot`), and loop-carried back-edges (`HYB-E3104`).

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn errors(r: &CheckResult, code: &str) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == code)
        .count()
}

#[test]
fn issue_2624_slot_loan_select_move_inside_closure_rejected() {
    let src = r#"struct acct { bal int }
fn main() {
    ch := make(chan *acct, 1)
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        s := &m["a"]
        select {
        ch <- move s:
            {}
        default:
            {}
        }
    }
    f()
}
"#;
    let r = check(src);
    assert!(errors(&r, "HYB-E3104") >= 1, "{:?}", r.diagnostics);
}

#[test]
fn issue_2624_slot_loan_alias_copy_move_rejected() {
    let src = r#"struct item {
    peer map[string]item
    n int
}
fn worker(p *item, done chan int) {
    p.peer["b"] = item{n: 2}
    done <- 1
}
fn main() {
    done := make(chan int, 1)
    m := map[string]item{}
    m["a"] = item{peer: m, n: 1}
    var slot *item
    slot = &m["a"]
    p := slot
    if false {
        slot.n = 2
    } else {
        task worker(move p, done)
    }
    <-done
}
"#;
    let r = check(src);
    assert!(errors(&r, "HYB-E3104") >= 1, "{:?}", r.diagnostics);
}

#[test]
fn issue_2624_slot_loan_loop_backedge_move_rejected() {
    let src = r#"struct acct { bal int }
fn worker(p *acct) {
    p.bal = 9
}
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var slot *acct
    for i in 0..2 {
        if i == 1 {
            task worker(move slot)
        }
        slot = &m["a"]
        slot.bal = i
    }
}
"#;
    let r = check(src);
    assert!(errors(&r, "HYB-E3104") >= 1, "{:?}", r.diagnostics);
}
