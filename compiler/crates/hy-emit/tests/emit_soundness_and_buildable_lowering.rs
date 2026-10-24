use hy_emit::{emit_go, emit_go_package};
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
fn package_export_collision_detects_initialism_splitting_issue_1421() {
    let src = "pub fn read_url() int { return 1 }\npub fn read_u_r_l() int { return 2 }\n";
    let ast = hy_syntax::parse_file("demo.hy", src).expect("parses");
    let err = emit_go_package(&ast, "demo", "demo").unwrap_err();
    assert!(
        err.message.contains("public functions `read_url` and `read_u_r_l` both normalize to exported Go identifier `ReadURL` in package mode"),
        "expected collision diagnostic, got: {}",
        err.message
    );

    // Declaration order must not change collision detection
    let src_rev = "pub fn read_u_r_l() int { return 2 }\npub fn read_url() int { return 1 }\n";
    let ast_rev = hy_syntax::parse_file("demo.hy", src_rev).expect("parses");
    let err_rev = emit_go_package(&ast_rev, "demo", "demo").unwrap_err();
    assert!(
        err_rev.message.contains("both normalize to exported Go identifier `ReadURL` in package mode"),
        "expected reverse collision diagnostic, got: {}",
        err_rev.message
    );
}

#[test]
fn package_export_collision_detects_repeated_underscores_issue_1421() {
    let src = "pub fn read_url() int { return 1 }\npub fn read__url() int { return 2 }\n";
    let ast = hy_syntax::parse_file("demo.hy", src).expect("parses");
    let err = emit_go_package(&ast, "demo", "demo").unwrap_err();
    assert!(
        err.message.contains("public functions `read_url` and `read__url` both normalize to exported Go identifier `ReadURL` in package mode"),
        "expected repeated underscore collision diagnostic, got: {}",
        err.message
    );
}

#[test]
fn package_export_allows_single_export_and_private_functions_issue_1421() {
    // Single export works
    let src_single = "pub fn read_url() int { return 1 }\n";
    let ast_single = hy_syntax::parse_file("demo.hy", src_single).expect("parses");
    let go_single = emit_go_package(&ast_single, "demo", "demo").expect("emits single export");
    assert!(go_single.contains("func ReadURL() int"));

    // Private functions are not exported, so distinct source names don't collide
    let src_priv = "fn read_url() int { return 1 }\nfn read_u_r_l() int { return 2 }\n";
    let ast_priv = hy_syntax::parse_file("demo.hy", src_priv).expect("parses");
    let go_priv = emit_go_package(&ast_priv, "demo", "demo").expect("emits private functions");
    assert!(go_priv.contains("func read_url() int"));
    assert!(go_priv.contains("func read_u_r_l() int"));

    // Binary mode (emit_go) does not export, so lowercase functions don't collide
    let src_bin = "pub fn read_url() int { return 1 }\npub fn read_u_r_l() int { return 2 }\nfn main() {}\n";
    let ast_bin = hy_syntax::parse_file("main.hy", src_bin).expect("parses");
    let go_bin = emit_go(&ast_bin).expect("emits binary mode");
    assert!(go_bin.contains("func read_url() int"));
    assert!(go_bin.contains("func read_u_r_l() int"));
}

