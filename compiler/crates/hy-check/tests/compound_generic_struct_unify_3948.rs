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
fn compound_generic_slice_unify_rejects_unmoved_task_spawn_3948() {
    let src = r#"
type t = int

struct box[u] {
    val u
}

fn first_elem[t](b box[[]t]) t {
    return b.val[0]
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    b := box[[]map[string]int]{val: []map[string]int{m}}
    done := make(chan int, 1)
    task worker(first_elem(b), done)
    m["main"] = 2
    <-done
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3001" || c == "HYB-E3003"),
        "expected HYB-E3001 or HYB-E3003 for task spawn sharing mutable map, got: {diag:?}"
    );
}

#[test]
fn compound_generic_array_unify_rejects_unmoved_task_spawn_3948() {
    let src = r#"
type t = int

struct box[u] {
    val u
}

fn first_elem_arr[t](b box[[1]t]) t {
    return b.val[0]
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    b := box[[1]map[string]int]{val: [1]map[string]int{m}}
    done := make(chan int, 1)
    task worker(first_elem_arr(b), done)
    <-done
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3001" || c == "HYB-E3003"),
        "expected HYB-E3001 or HYB-E3003 for task spawn sharing mutable map via array box, got: {diag:?}"
    );
}

#[test]
fn compound_generic_nested_inner_unify_rejects_unmoved_task_spawn_3948() {
    let src = r#"
type t = int

struct inner[v] {
    item v
}

struct box[u] {
    val u
}

fn unwrap_nested[t](b box[inner[t]]) t {
    return b.val.item
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    b := box[inner[map[string]int]]{val: inner[map[string]int]{item: m}}
    done := make(chan int, 1)
    task worker(unwrap_nested(b), done)
    <-done
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3001" || c == "HYB-E3003"),
        "expected HYB-E3001 or HYB-E3003 for task spawn sharing mutable map via nested inner box, got: {diag:?}"
    );
}

#[test]
fn compound_generic_option_unify_rejects_unmoved_task_spawn_3948() {
    let src = r#"
type t = int

struct box[u] {
    val u
}

fn unwrap_box[t](b box[option[t]]) t {
    return b.val.unwrap()
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    b := box[option[map[string]int]]{val: some(m)}
    done := make(chan int, 1)
    task worker(unwrap_box(b), done)
    <-done
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E3001" || c == "HYB-E3003"),
        "expected HYB-E3001 or HYB-E3003 for task spawn sharing mutable map via option box, got: {diag:?}"
    );
}

#[test]
fn compound_generic_sendable_primitive_accepts_cleanly_3948() {
    let src = r#"
type t = int

struct box[u] {
    val u
}

fn first_elem[t](b box[[]t]) t {
    return b.val[0]
}

fn worker(n int, done chan int) {
    println(n)
    done <- 1
}

fn main() {
    b := box[[]int]{val: []int{42}}
    done := make(chan int, 1)
    task worker(first_elem(b), done)
    <-done
}
"#;
    let diag = codes(src);
    assert!(
        diag.is_empty(),
        "expected 0 diagnostics for passing sendable int from generic box, got: {diag:?}"
    );
}
