use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn constconv_folds_negative_parenthesized_and_64bit_literals() {
    let go = emit_src(
        "\
fn main() {
    a := u8(-1)
    b := i8(-200)
    c := u8((300))
    d := u64(-1)
    fmt.println(\"{a} {b} {c} {d}\")
}
",
    );
    assert!(
        go.contains("a := uint8(255)"),
        "expected uint8(255) for u8(-1), got:\n{go}"
    );
    assert!(
        go.contains("b := int8(56)"),
        "expected int8(56) for i8(-200), got:\n{go}"
    );
    assert!(
        go.contains("c := uint8(44)"),
        "expected uint8(44) for u8((300)), got:\n{go}"
    );
    assert!(
        go.contains("d := uint64(18446744073709551615)"),
        "expected folded uint64 max for u64(-1), got:\n{go}"
    );
}

#[test]
fn rangebind_elides_shadowed_loop_or_select_bindings() {
    // Inner `k := 1` and inner `for i in ...` shadow the outer loop bindings,
    // so the outer range header must lower the unread outer slot to `_`.
    let go = emit_src(
        "\
fn main() {
    xs := []int{10, 20}
    for k, v in xs {
        k := 1
        fmt.println(\"{k} {v}\")
    }
    for i, v in xs {
        for i in xs {
            fmt.println(\"{i}\")
        }
        fmt.println(\"{v}\")
    }
}
",
    );
    assert!(
        go.contains("for _, v := range xs"),
        "expected shadowed first binding to lower to `_`, got:\n{go}"
    );
    assert!(
        !go.contains("for k, v := range xs") && !go.contains("for i, v := range xs"),
        "unexpectedly kept shadowed loop binding in:\n{go}"
    );
}

#[test]
fn duration_precedence_and_double_negation_emit_valid_go() {
    let go = emit_src(
        "\
fn main() {
    ratio := time.seconds(10) / time.seconds(2)
    x := 5
    neg := - -x
    fmt.println(\"{ratio} {neg}\")
}
",
    );
    assert!(
        go.contains("time.Duration(10) * time.Second / (time.Duration(2) * time.Second)"),
        "expected parenthesized RHS duration in division, got:\n{go}"
    );
    assert!(
        !go.contains("--x") && go.contains("-(-x)"),
        "expected `-(-x)` instead of invalid Go `--x`, got:\n{go}"
    );
}

#[test]
fn emit_fn_clears_var_tables_registers_receiver_and_respects_shadowed_pkg_roots() {
    let go = emit_src(
        "\
struct pool_stats {
    mu sync.mutex
}

fn (s *pool_stats) touch() {
    s.mu.lock()
    s.mu.unlock()
}

struct timer {
    code int
}

fn check_local() int {
    os := timer{code: 7}
    return os.code
}
",
    );
    assert!(
        go.contains("s.mu.Lock()") && go.contains("s.mu.Unlock()"),
        "expected method receiver `s` to resolve `s.mu` as sync.mutex (`Lock()`/`Unlock()`), got:\n{go}"
    );
    assert!(
        !go.contains("\"os\""),
        "local struct variable `os` must not trigger an unused `import \"os\"`, got:\n{go}"
    );
}

#[test]
fn task_spawn_normalizes_foreign_and_pointer_calls() {
    let go = emit_src(
        "\
struct env {
    id int
}

fn touch(e *env) {
    fmt.println(\"{e.id}\")
}

fn main() {
    e := env{id: 1}
    task touch(e)
}
",
    );
    assert!(
        go.contains("go touch(&e)"),
        "expected `task touch(e)` to auto-address pointer parameter via emit_call (`go touch(&e)`), got:\n{go}"
    );
}

#[test]
fn inner_block_local_vars_do_not_leak_to_sibling_pkg_references() {
    let go = emit_src(
        "\
struct timer {
    code int
}

fn main() {
    {
        os := timer{code: 7}
        fmt.println(\"{os.code}\")
    }
    fmt.println(\"{len(os.args)}\")
}
",
    );
    assert!(
        go.contains("\"os\"") && go.contains("len(os.Args)"),
        "expected sibling reference after block exit to resolve package `os` (`os.Args`), got:\n{go}"
    );
}

#[test]
fn match_elides_unread_or_blank_some_bindings_and_task_hoists_pre_try_args() {
    let go = emit_src(
        "\
fn first() int {
    return 1
}

fn fallible() (int, error) {
    return 2, nil
}

fn worker(a int, b int) {
    fmt.println(\"{a} {b}\")
}

fn check_match(opt option[int]) int {
    match opt {
        some(_): {
            return 1
        }
        none: {
            return 0
        }
    }
}

fn main() {
    task worker(first(), fallible()?)
}
",
    );
    assert!(
        !go.contains("_ :="),
        "expected `some(_):` to elide invalid `_ :=` binding, got:\n{go}"
    );
    assert!(
        go.contains("hyT0 := first()") && go.contains("go worker(hyT0, hyT1)"),
        "expected `task worker(first(), fallible()?)` to hoist `first()` once into `hyT0` and pass `hyT0` to `go worker`, got:\n{go}"
    );
}

#[test]
fn match_multi_guarded_arms_isolate_bindings_and_hoist_second_guard_try() {
    let go = emit_src(
        "\
fn check_ok(v int) (bool, error) {
    return v > 0, nil
}

fn eval_guarded(opt option[int]) (int, error) {
    limit := 100
    match opt {
        some(x) if x < limit: {
            return x, nil
        }
        some(limit) if check_ok(limit)?: {
            return limit, nil
        }
        some(z): {
            return z + limit, nil
        }
        none: {
            return 0, nil
        }
    }
}

fn eval_all_guarded(opt option[int]) (int, error) {
    limit := 100
    match opt {
        some(limit) if limit > 50: {
            return limit, nil
        }
        some(x) if check_ok(x + limit)?: {
            return x, nil
        }
        none: {
            return 0, nil
        }
    }
}
",
    );
    let first_guard = go.find("if x < limit {").expect("first arm guard");
    let second_bind = go
        .find("limit := hyMatch0.val")
        .expect("second arm limit binding");
    assert!(
        first_guard < second_bind,
        "expected second arm `limit := hyMatch0.val` to be scoped after first arm `if x < limit`, got:\n{go}"
    );
    assert!(
        go.contains("hyT2, hyErr3 := check_ok(limit)")
            && go.contains("hyT2, hyErr3 := check_ok(x + limit)"),
        "expected `?` in second match guard to hoist cleanly in both unguarded-fallback and all-guarded forms, got:\n{go}"
    );
}

#[test]
fn scoped_type_hints_restore_after_inner_block_and_closure_shadowing() {
    let go = emit_src(
        "\
struct plain_box {
    milliseconds int
}

fn main() {
    t0 := time.now()
    x := time.since(t0)
    {
        x := plain_box{milliseconds: 7}
        fmt.println(\"{x.milliseconds}\")
    }
    f := fn(x plain_box) int {
        return x.milliseconds
    }
    fmt.println(\"{f(plain_box{milliseconds: 3})} {x.milliseconds}\")
}
",
    );
    assert!(
        go.contains("fmt.Sprintf(\"%v\", x.milliseconds)")
            && go.contains("return x.milliseconds")
            && go.contains("x.Milliseconds()"),
        "expected shadowed inner block and closure param to emit `x.milliseconds` while outer duration binding restores `x.Milliseconds()`, got:\n{go}"
    );
}

#[test]
fn float_trailing_dot_emits_valid_statements_issue_659() {
    let go = emit_src(
        "\
package main

import \"fmt\"

fn main() {
    var scale f64 = 2.
    fmt.println(scale)
    fmt.println(scale * 3.0)
}
",
    );
    assert!(
        go.contains("var scale float64 = 2."),
        "expected var scale float64 = 2., got:\n{go}"
    );
    assert!(
        go.contains("fmt.Println(scale)"),
        "expected fmt.Println(scale) as its own statement, got:\n{go}"
    );
    assert!(
        !go.contains("2.fmt"),
        "must not fold 2. into 2.fmt, got:\n{go}"
    );
}


#[test]
fn test_channel_receive_and_range_element_propagation_issue_1456() {
    let src = r#"
import "os"

struct holder { ch chan int }
struct node { v int }
fn take_ptr(n *node) { _ = n.v }

fn run(outer chan chan int, files chan *os.file, nodes chan *node, h holder) {
    c1 := <-outer
    for x in c1 { _ = x }
    select {
        f := <-files: { _ = f.close() }
        n := <-nodes: { take_ptr(n) }
        default: {}
    }
    for w in <-outer { _ = w }
    for q in h.ch { _ = q }
}

fn main() {}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("for x := range c1 {"),
        "expected single-variable range for received channel c1, got:\n{go}"
    );
    assert!(
        !go.contains("for _, x := range c1 {"),
        "unexpected two-variable range for received channel c1, got:\n{go}"
    );
    assert!(
        go.contains("case f := <-files:"),
        "expected select receive for files, got:\n{go}"
    );
    assert!(
        go.contains("_ = f.Close()"),
        "expected normalized Close() method for foreign file handle, got:\n{go}"
    );
    assert!(
        go.contains("take_ptr(n)"),
        "expected pointer arg passed directly to take_ptr, got:\n{go}"
    );
    assert!(
        !go.contains("take_ptr(&n)"),
        "must not double-address pointer variable &n, got:\n{go}"
    );
    assert!(
        go.contains("for w := range <-outer {"),
        "expected single-variable range for <-outer, got:\n{go}"
    );
    assert!(
        !go.contains("for _, w := range <-outer {"),
        "unexpected two-variable range for <-outer, got:\n{go}"
    );
    assert!(
        go.contains("for q := range h.ch {"),
        "expected single-variable range for struct channel field h.ch, got:\n{go}"
    );
    assert!(
        !go.contains("for _, q := range h.ch {"),
        "unexpected two-variable range for struct channel field h.ch, got:\n{go}"
    );
}
