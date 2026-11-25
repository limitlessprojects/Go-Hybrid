use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn assign_field_target_index_call_mutating_loaned_map_emits_hyb_e3102() {
    let src = r#"
struct container {
    val int
}

fn mutate(m map[string]int) int {
    m["b"] = 2
    return 0
}

fn bad() {
    m := map[string]int{"a": 1}
    slot := &m["a"]
    var arr [1]container
    arr[mutate(m)].val = 99
    println(slot)
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for call mutating loaned map inside field-assign target index, got: {diag:?}"
    );
}

#[test]
fn assign_nested_index_target_call_mutating_loaned_map_emits_hyb_e3102() {
    let src = r#"
fn mutate(m map[string]int) int {
    m["b"] = 2
    return 0
}

fn bad() {
    m := map[string]int{"a": 1}
    slot := &m["a"]
    var mat [1][1]int
    mat[mutate(m)][0] = 99
    println(slot)
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for call mutating loaned map inside outer index of nested assign target, got: {diag:?}"
    );
}

#[test]
fn assign_target_index_closure_const_mutation_emits_hyb_e3006() {
    let src = r#"
struct container {
    val int
}

fn apply(f fn() int) int {
    return f()
}

fn bad() {
    const xs = []int{1, 2}
    var arr [1]container
    arr[apply(fn() int {
        xs[0] = 9
        return 0
    })].val = 99
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for const slice mutation inside assign target index closure, got: {diag:?}"
    );
}

#[test]
fn assign_target_index_closure_in_nested_scope_captures_block_local_for_boundary_check() {
    let src = r#"
struct container {
    val int
}

fn apply(f fn() int) int {
    return f()
}

fn bad(ch chan []int) {
    var arr [1]container
    if true {
        s := []int{1, 2}
        arr[apply(fn() int {
            ch <- s
            return 0
        })].val = 99
    }
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 for non-move send of block-local slice captured inside assign target index closure, got: {diag:?}"
    );
}
