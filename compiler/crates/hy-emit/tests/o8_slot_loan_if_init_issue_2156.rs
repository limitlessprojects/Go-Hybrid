//! Regression test for issue #2156:
//! [soundness] hy-emit: O8 slot loans in if <init>; <cond> fail go build (Row 13)
//! or emit post-spawn map data races and wrong-key store-backs.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("issue_2156.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

fn between<'a>(go: &'a str, from: &str, to: &str) -> &'a str {
    let start = go.find(from).expect("has start marker") + from.len();
    let end = go[start..]
        .find(to)
        .map(|e| start + e)
        .expect("has end marker");
    &go[start..end]
}

#[test]
fn o8_slot_loan_if_init_emits_value_copy_and_store_back_repro1() {
    let src = r#"
struct item {
    n int
}

fn main() {
    m := map[string]item{"a": item{n: 1}}
    if p := &m["a"]; p.n > 0 {
        p.n = 7
    }
    fmt.println(m["a"].n)
}
"#;
    let go = emit(src);
    assert!(
        !go.contains(r#"&m["a"]"#),
        "raw map index address &m[\"a\"] must not be emitted:\n{go}"
    );
    assert!(
        go.contains(r#"p := m["a"]"#),
        "slot loan should emit value copy p := m[\"a\"]:\n{go}"
    );
    assert!(
        go.contains(r#"m["a"] = p"#),
        "slot loan must emit store-back m[\"a\"] = p:\n{go}"
    );
}

#[test]
fn o8_slot_loan_else_if_init_emits_value_copy_and_store_back() {
    let src = r#"
struct item {
    n int
}

fn main() {
    m := map[string]item{"a": item{n: 1}, "b": item{n: 2}}
    if false {
        fmt.println(0)
    } else if q := &m["b"]; q.n > 0 {
        q.n = 8
    }
    fmt.println(m["b"].n)
}
"#;
    let go = emit(src);
    assert!(
        !go.contains(r#"&m["b"]"#),
        "raw map index address &m[\"b\"] must not be emitted in else-if:\n{go}"
    );
    assert!(
        go.contains(r#"q := m["b"]"#),
        "else-if slot loan should emit value copy q := m[\"b\"]:\n{go}"
    );
    assert!(
        go.contains(r#"m["b"] = q"#),
        "else-if slot loan must emit store-back m[\"b\"] = q:\n{go}"
    );
}

#[test]
fn o8_slot_loan_struct_field_if_init_emits_value_copy() {
    let src = r#"
struct item {
    n int
}

struct box_item {
    it item
}

fn main() {
    b := box_item{it: item{n: 5}}
    if r := &b.it; r.n > 0 {
        r.n = 11
    }
    fmt.println(b.it.n)
}
"#;
    let go = emit(src);
    assert!(
        go.contains("r := b.it"),
        "struct field slot loan should emit value copy r := b.it:\n{go}"
    );
    assert!(
        go.contains("b.it = r"),
        "struct field slot loan must emit store-back b.it = r:\n{go}"
    );
}

#[test]
fn o8_slot_loan_if_init_with_try_in_cond_snapshots_key_and_hands_off_store_back_repro2() {
    let src = r#"
struct item {
    n int
}

fn ok(v int) (bool, error) {
    return v > 0, nil
}

fn worker(m map[string]item, done chan int) {
    for i in 0..1000 {
        m["w"] = item{n: i}
    }
    done <- m["a"].n
}

fn run() (int, error) {
    m := map[string]item{"a": item{n: 1}, "b": item{n: 2}}
    k := "a"
    if p := &m[k]; ok(p.n)? {
        k = "b"
        p.n = 7
    }
    done := make(chan int, 1)
    if p := &m["a"]; ok(p.n)? {
        p.n = 9
        task worker(move m, done)
    } else {
        done <- 0
    }
    return <-done, nil
}

fn main() {
    _, _ = run()
}
"#;
    let go = emit(src);

    // 1. Key snapshot check: k is mutated inside then arm, so key must be snapshotted
    assert!(
        go.contains("hyKey0 := k") || go.contains("hyKey"),
        "key must be snapshotted when mutated inside if arm:\n{go}"
    );
    assert!(
        !go.contains("m[k] = p"),
        "must not store back to mutated variable k:\n{go}"
    );

    // 2. Concurrency / move boundary check: store-back must NOT emit after go worker(m, done)
    assert!(
        go.contains("go worker(m, done)"),
        "task worker must be lowered to go worker:\n{go}"
    );
    let then_arm = between(&go, "p.n = 9", "go worker(m, done)");
    assert!(
        then_arm.contains(r#"m["a"] = p"#),
        "store-back must be handed off before go worker(m, done) inside then arm:\n{go}"
    );
    assert!(
        !between(&go, "done <- 0", "return <-done, nil").contains(r#"m["a"] = p"#),
        "store-back must not emit after the whole if block on the moved path:\n{go}"
    );
}
