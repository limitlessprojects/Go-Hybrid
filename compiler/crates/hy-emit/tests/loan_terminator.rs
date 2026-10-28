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
fn slot_loan_flushes_between_operand_eval_and_return_issue_1398() {
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
    let ast = parse_file("loan_return_operand.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let eval_pos = go
        .find("hyRet0 := c.next()")
        .expect("operand evaluated into a temp before the store-back");
    let store_pos = go.find("m[\"k\"] = c").expect("has store-back");
    let ret_pos = go.find("return hyRet0").expect("returns the temp");
    assert!(
        eval_pos < store_pos && store_pos < ret_pos,
        "expected eval < store-back < return, got:\n{}",
        go
    );
}

#[test]
fn slot_loan_store_back_emits_before_propagate_error_return_issue_1398() {
    let src = "\
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
    let ast = parse_file("loan_return_propagate.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let store_pos = go
        .find("m[\"k\"] = c")
        .expect("store-back emitted on the propagate-error path");
    let ret_pos = go
        .find("return 0, hyErr0")
        .expect("has propagate-error early return");
    assert!(
        store_pos < ret_pos,
        "expected store-back before the propagate-error return, got:\n{}",
        go
    );
}
