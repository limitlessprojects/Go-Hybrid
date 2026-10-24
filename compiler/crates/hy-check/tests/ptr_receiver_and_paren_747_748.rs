use hy_check::check_file;
use hy_syntax::parse_file;

fn check_src(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parse");
    let res = check_file(&file);
    res.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn issue_747_pointer_receiver_method_on_const_struct_rejected() {
    let src = r#"
struct counter {
    n int
}

fn (c *counter) bump() {
    c.n = c.n + 1
}

const global_c = counter{n: 0}

fn main() {
    global_c.bump()
}
"#;
    let codes = check_src(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for pointer-receiver method call on const struct, got {:?}",
        codes
    );
}

#[test]
fn issue_747_task_spawn_pointer_receiver_method_on_const_and_local_rejected() {
    let src = r#"
struct counter {
    n int
}

fn (c *counter) bump() {
    c.n = c.n + 1
}

const global_c = counter{n: 0}

fn main() {
    task global_c.bump()
}
"#;
    let codes = check_src(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006" || c == "HYB-E3001"),
        "expected HYB-E3006 / HYB-E3001 for task global_c.bump(), got {:?}",
        codes
    );

    let src_local = r#"
struct counter {
    n int
}

fn (c *counter) bump() {
    c.n = c.n + 1
}

fn main() {
    var c = counter{n: 0}
    task c.bump()
}
"#;
    let codes_local = check_src(src_local);
    assert!(
        codes_local.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for task c.bump() auto-addressing mutable local struct, got {:?}",
        codes_local
    );
}

#[test]
fn issue_747_closure_body_const_method_call_and_task_spawn_rejected() {
    let src = r#"
struct counter {
    n int
}

fn (c *counter) bump() {
    c.n = c.n + 1
}

const global_c = counter{n: 0}

fn main() {
    f := fn() {
        global_c.bump()
        task global_c.bump()
    }
    f()
}
"#;
    let codes = check_src(src);
    let e3006_count = codes.iter().filter(|c| *c == "HYB-E3006").count();
    assert!(
        e3006_count >= 2,
        "expected at least 2 HYB-E3006 diagnostics inside closure body, got {:?}",
        codes
    );
}

#[test]
fn issue_748_parenthesized_call_and_addr_deref_on_const_rejected() {
    let src = r#"
const global_m = map[string]int{"k": 1}

fn main() {
    (delete)(global_m, "k")
}
"#;
    let codes = check_src(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for (delete)(global_m, \"k\"), got {:?}",
        codes
    );

    let src_deref = r#"
struct counter {
    n int
}

const global_c = counter{n: 0}

fn main() {
    p := (&global_c)
    p.n = 99
}
"#;
    let codes_deref = check_src(src_deref);
    assert!(
        codes_deref.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for p := (&global_c); p.n = 99, got {:?}",
        codes_deref
    );
}

#[test]
fn issue_748_parenthesized_builtin_loan_and_borrow_invalidation_rejected() {
    let src = r#"
struct box_t {
    n int
}

fn main() {
    var m = map[string]box_t{"a": box_t{n: 1}}
    p := &m["a"]
    (delete)(m, "a")
    _ = p.n
}
"#;
    let codes = check_src(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for (delete)(m, \"a\") while slot loan is live, got {:?}",
        codes
    );
}
