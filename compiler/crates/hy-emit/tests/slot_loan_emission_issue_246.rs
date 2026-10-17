use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parse");
    emit_go(&file).expect("emit")
}

#[test]
fn slot_loan_flushes_at_nonlexical_last_use_before_same_block_read_delete_or_move() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn worker(m map[string]service, done chan<- bool) {\n    done <- true\n}\n\nfn main() {\n    m := map[string]service{\"api\": {replicas: 1}}\n    s := &m[\"api\"]\n    s.replicas = 5\n    fmt.println(m[\"api\"].replicas)\n    delete(m, \"api\")\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    <-done\n}\n",
    );
    let mut_at = go.find("s.replicas = 5").expect("loan mutation");
    let flush_at = go.find("m[\"api\"] = s").expect("store-back emitted");
    let read_at = go
        .find("fmt.Println(m[\"api\"].replicas)")
        .expect("subsequent read emitted");
    let del_at = go.find("delete(m, \"api\")").expect("delete emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        mut_at < flush_at && flush_at < read_at && read_at < del_at && del_at < spawn_at,
        "slot loan must flush right after non-lexical last_use before same-block read/delete/move:\n{}",
        go
    );
    assert_eq!(
        go.matches("m[\"api\"] = s").count(),
        1,
        "must not emit duplicate store-back at block end:\n{}",
        go
    );
}

#[test]
fn slot_loan_rebind_flushes_previous_loan_before_overwriting() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn main() {\n    m1 := map[string]service{\"a\": {replicas: 1}}\n    m2 := map[string]service{\"b\": {replicas: 2}}\n    s := &m1[\"a\"]\n    s.replicas = 10\n    s = &m2[\"b\"]\n    s.replicas = 20\n}\n",
    );
    let mut_a = go.find("s.replicas = 10").expect("first mutation");
    let flush_a = go.find("m1[\"a\"] = s").expect("first store-back");
    let rebind_b = go.find("s = m2[\"b\"]").expect("rebind to m2");
    let mut_b = go.find("s.replicas = 20").expect("second mutation");
    let flush_b = go.find("m2[\"b\"] = s").expect("second store-back");
    assert!(
        mut_a < flush_a && flush_a < rebind_b && rebind_b < mut_b && mut_b < flush_b,
        "rebind must flush m1[\"a\"] = s before overwriting s with m2[\"b\"]:\n{}",
        go
    );
    assert_eq!(
        go.matches("m1[\"a\"] = s").count(),
        1,
        "m1[\"a\"] flushed once:\n{}",
        go
    );
}

#[test]
fn slot_loan_snapshots_mutated_or_nonidempotent_index_key() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn next_key() string {\n    return \"a\"\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}, \"b\": {replicas: 2}}\n    k := \"a\"\n    s := &m[k]\n    k = \"b\"\n    s.replicas = 10\n    m2 := map[string]service{\"a\": {replicas: 1}}\n    s2 := &m2[next_key()]\n    s2.replicas = 30\n}\n",
    );
    assert!(
        go.contains("hyKey0 := k")
            && go.contains("s := m[hyKey0]")
            && go.contains("m[hyKey0] = s"),
        "mutated index variable k must be snapshotted into hyKey0:\n{}",
        go
    );
    assert!(
        go.contains("hyKey1 := next_key()")
            && go.contains("s2 := m2[hyKey1]")
            && go.contains("m2[hyKey1] = s2"),
        "call index expression next_key() must be evaluated once via hyKey1:\n{}",
        go
    );
}

#[test]
fn slot_loan_in_match_some_arm_flushes_inside_arm_scope() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn maybe_key() option[string] {\n    return some(\"a\")\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    match maybe_key() {\n        some(k): {\n            s := &m[k]\n            s.replicas = 10\n        }\n        none: {\n        }\n    }\n}\n",
    );
    let ok_branch = go.find("if hyMatch0.ok {").expect("some arm branch");
    let flush_at = go.find("m[k] = s").expect("store-back in some arm");
    let else_branch = go.find("} else {").expect("else branch");
    assert!(
        ok_branch < flush_at && flush_at < else_branch,
        "some(k) arm slot loan must flush inside `if hyMatch0.ok` before `}} else {{`:\n{}",
        go
    );
}

#[test]
fn slot_loan_parallel_define_assign_and_try_in_place() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn get_key() (string, error) {\n    return \"a\", nil\n}\n\nfn main() {\n    m1 := map[string]service{\"a\": {replicas: 1}}\n    m2 := map[string]service{\"b\": {replicas: 2}}\n    s1, s2 := &m1[get_key()?], &m2[\"b\"]\n    s1.replicas = 10\n    s2.replicas = 20\n}\n",
    );
    assert!(
        !go.contains("&m1") && !go.contains("&m2"),
        "parallel slot loans with `?` must strip `&` and lower to copy-read + store-back:\n{}",
        go
    );
    assert!(
        go.contains("s1.replicas = 10") && go.contains("m2[\"b\"] = s2"),
        "parallel slot loans must emit store-backs for both bindings:\n{}",
        go
    );
}
