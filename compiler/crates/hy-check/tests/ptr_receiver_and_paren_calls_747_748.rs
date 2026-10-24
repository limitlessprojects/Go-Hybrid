use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("unexpected parse diagnostics: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn pointer_receiver_const_writes_and_task_spawns_747() {
    let src_const_ptr_recv = r#"package main
struct counter { n int }
fn (c *counter) bump() { c.n = c.n + 1 }
const global_c = counter{n: 0}
fn main() {
    global_c.bump()
}
"#;
    let codes = check_codes(src_const_ptr_recv);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for pointer-receiver method mutating const struct, got {:?}",
        codes
    );

    let src_spawn_ptr_recv = r#"package main
struct counter { n int }
fn (c *counter) bump() { c.n = c.n + 1 }
fn main() {
    var c = counter{n: 0}
    task c.bump()
    c.bump()
}
"#;
    let codes_spawn = check_codes(src_spawn_ptr_recv);
    assert!(
        codes_spawn.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for spawning pointer-receiver method on local struct, got {:?}",
        codes_spawn
    );
}

#[test]
fn parenthesized_calls_and_addr_of_const_writes_748() {
    let src_paren_delete_const = r#"package main
const global_m = map[string]int{"a": 1, "b": 2}
fn main() {
    (delete)(global_m, "a")
    (clear)(global_m)
}
"#;
    let codes_const = check_codes(src_paren_delete_const);
    assert!(
        codes_const.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for parenthesized (delete)/(clear) on const map, got {:?}",
        codes_const
    );

    let src_paren_delete_a3 = r#"package main
struct item { v int }
fn main() {
    var m = map[string]item{"a": item{v: 10}}
    s := &m["a"]
    (delete)(m, "a")
    s.v = 99
}
"#;
    let codes_a3 = check_codes(src_paren_delete_a3);
    assert!(
        codes_a3.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for parenthesized (delete) inside live &m[k] loan window, got {:?}",
        codes_a3
    );

    let src_paren_copy_share = r#"package main
fn sink(xs []map[string]int) {}
fn main() {
    src := []map[string]int{map[string]int{"a": 1}}
    dst := []map[string]int{map[string]int{"b": 2}}
    (copy)(dst, src)
    task sink(move src)
    dst[0]["a"] = 99
}
"#;
    let codes_copy = check_codes(src_paren_copy_share);
    assert!(
        codes_copy.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for parenthesized (copy) sharing storage across move, got {:?}",
        codes_copy
    );
}

#[test]
fn negative_control_value_receiver_and_pure_paren_calls_accept() {
    let src_ok = r#"package main
struct counter { n int }
fn (c counter) read() int { return c.n }
const global_c = counter{n: 5}
fn main() {
    _ = (global_c.read)()
    _ = (len)([]int{1, 2, 3})
}
"#;
    let codes = check_codes(src_ok);
    assert!(
        codes.is_empty(),
        "expected 0 diagnostics for value-receiver read on const struct and pure parenthesized calls, got {:?}",
        codes
    );
}
