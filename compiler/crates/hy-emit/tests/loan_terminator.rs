use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn slot_loan_store_back_emits_before_valued_return_issue_203() {
    let src = "\
fn update(m map[string]int, k string) int {
    slot := &m[k]
    slot = 42
    return slot
}
fn main() {
    m := map[string]int{\"k\": 0}
    _ = update(m, \"k\")
}
";
    let ast = parse_file("loan_return.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let store_pos = go.find("m[k] = slot").expect("has store-back");
    let ret_pos = go.find("return slot").expect("has return");
    assert!(
        store_pos < ret_pos,
        "expected store-back before return slot, got:\n{}",
        go
    );
    assert_eq!(
        go.matches("m[k] = slot").count(),
        1,
        "expected no trailing duplicate store-back after return, got:\n{}",
        go
    );
}

#[test]
fn slot_loan_store_back_emits_before_early_void_return_issue_203() {
    let src = "\
fn bump_if_present(m map[string]int, k string) {
    if true {
        slot := &m[k]
        slot = 42
        return
    }
}
fn main() {
    m := map[string]int{\"k\": 0}
    bump_if_present(m, \"k\")
}
";
    let ast = parse_file("loan_lost_write.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let store_pos = go.find("m[k] = slot").expect("has store-back");
    let ret_pos = go.find("\t\treturn").expect("has return");
    assert!(
        store_pos < ret_pos,
        "expected store-back before early return, got:\n{}",
        go
    );
    assert_eq!(
        go.matches("m[k] = slot").count(),
        1,
        "expected no unreachable trailing store-back after return, got:\n{}",
        go
    );
}

#[test]
fn slot_loan_store_back_emits_before_break_and_continue_issue_203() {
    let src = "\
fn main() {
    m := map[string]int{\"k\": 0}
    for i in 0..2 {
        slot := &m[\"k\"]
        slot = i + 10
        break
    }
}
";
    let ast = parse_file("loan_break.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let store_pos = go.find("m[\"k\"] = slot").expect("has store-back");
    let brk_pos = go.find("\t\tbreak").expect("has break");
    assert!(
        store_pos < brk_pos,
        "expected store-back before break, got:\n{}",
        go
    );
    assert_eq!(
        go.matches("m[\"k\"] = slot").count(),
        1,
        "expected no unreachable trailing store-back after break, got:\n{}",
        go
    );
}

#[test]
fn slot_loan_evaluates_return_operand_before_store_back_issue_1398() {
    let src = "\
struct counter {
    n int
}
fn (c *counter) next() int {
    c.n = c.n + 1
    return c.n
}
fn bump(m map[string]counter) int {
    c := &m[\"k\"]
    return c.next()
}
fn main() {
    m := map[string]counter{\"k\": counter{n: 10}}
    _ = bump(m)
}
";
    let ast = parse_file("loan_return_eval.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let bump_start = go.find("func bump").expect("has bump");
    let bump_body = &go[bump_start..];
    let eval_pos = bump_body.find("c.next()").expect("evaluates call");
    let store_pos = bump_body.find("m[\"k\"] = c").expect("has store-back");
    let ret_pos = bump_body.find("return hyRet0").expect("has return");
    assert!(
        eval_pos < store_pos,
        "expected c.next() call to be evaluated before store-back m[\"k\"] = c, got:\n{}",
        bump_body
    );
    assert!(
        store_pos < ret_pos,
        "expected store-back before return, got:\n{}",
        bump_body
    );
}

#[test]
fn slot_loan_store_back_emits_on_try_early_return_issue_1398() {
    let src = "\
import \"errors\"

struct counter {
    n int
}

fn fail_step() (int, error) {
    return 0, errors.new(\"boom\")
}

fn update_then_try(m map[string]counter) (int, error) {
    c := &m[\"k\"]
    c.n = 42
    x := fail_step()?
    c.n = c.n + x
    return c.n, nil
}

fn main() {
    m := map[string]counter{\"k\": counter{n: 10}}
    _, _ = update_then_try(m)
}
";
    let ast = parse_file("loan_try_early_return.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let err_branch = go.find("if hyErr0 != nil {").expect("has error check");
    let ret_pos = go.find("return 0, hyErr0").expect("has error return");
    // Assert that the store-back m["k"] = c occurs inside the error branch before returning
    let store_in_err = go[err_branch..ret_pos].contains("m[\"k\"] = c");
    assert!(
        store_in_err,
        "expected store-back inside error propagation if-branch before return 0, hyErr0, got:\n{}",
        go
    );
}
