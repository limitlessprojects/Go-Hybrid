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
fn local_closure_call_with_mismatched_arg_type_emits_hyb_e1002() {
    let src = r#"
fn main() {
    f := fn(x int) {
        println(x)
    }
    f("not-an-int")
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for passing string to int-typed closure parameter, got: {diag:?}"
    );
}

#[test]
fn struct_fn_field_call_with_mismatched_arg_type_emits_hyb_e1002() {
    let src = r#"
struct runner {
    cb fn(x int)
}

fn main() {
    f := fn(x int) {
        println(x)
    }
    r := runner{cb: f}
    r.cb("wrong")
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for passing string to int-typed struct fn field, got: {diag:?}"
    );
}

#[test]
fn task_spawn_on_non_callable_local_emits_hyb_e1002() {
    let src = r#"
fn main() {
    not_a_fn := 42
    task not_a_fn()
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for spawning task on int variable, got: {diag:?}"
    );
}

#[test]
fn task_spawn_on_closure_with_wrong_arity_and_arg_types_emits_hyb_e1002() {
    let src = r#"
fn main() {
    f := fn(x int) {
        println(x)
    }
    task f("wrong", 123)
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for task spawn on closure with mismatched arity/arg types, got: {diag:?}"
    );
}

#[test]
fn task_spawn_on_closure_with_mismatched_arg_type_emits_hyb_e1002() {
    let src = r#"
fn main() {
    f := fn(x int) {
        println(x)
    }
    task f("wrong")
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for task spawn on closure with string instead of int, got: {diag:?}"
    );
}

#[test]
fn task_spawn_on_struct_fn_field_with_mismatched_arg_type_emits_hyb_e1002() {
    let src = r#"
struct runner {
    cb fn(x int)
}

fn main() {
    f := fn(x int) {
        println(x)
    }
    r := runner{cb: f}
    task r.cb("wrong")
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for task spawn on struct fn field with string instead of int, got: {diag:?}"
    );
}

#[test]
fn task_spawn_on_non_callable_struct_field_emits_hyb_e1002() {
    let src = r#"
struct item {
    val int
}

fn main() {
    it := item{val: 10}
    task it.val()
}
"#;
    let diag = codes(src);
    assert!(
        diag.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for task spawn on int field, got: {diag:?}"
    );
}

#[test]
fn valid_closure_and_task_calls_accept_cleanly() {
    let src = r#"
struct runner {
    cb fn(x int)
}

fn worker(n int) {
    println(n)
}

fn main() {
    f := fn(x int) {
        println(x)
    }
    f(42)

    r := runner{cb: f}
    r.cb(100)

    task worker(400)
}
"#;
    let diag = codes(src);
    assert!(
        diag.is_empty(),
        "expected 0 diagnostics for valid closure calls and task spawns, got: {diag:?}"
    );
}
