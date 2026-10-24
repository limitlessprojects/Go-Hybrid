use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|e| panic!("parse failed: {:?}\nsource:\n{}", e, src));
    emit_go(&ast).unwrap_or_else(|e| panic!("emit failed: {:?}\nsource:\n{}", e, src))
}

fn emit_err(src: &str) -> String {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|e| panic!("parse failed: {:?}\nsource:\n{}", e, src));
    emit_go(&ast)
        .expect_err("expected emit error")
        .message
}

#[test]
fn fnlit_return_none_and_bare_return_lower_with_closure_scoped_results() {
    let go = emit(
        r#"
fn outer() int {
    f := fn(flag bool) option[u8] {
        if !flag {
            return none
        }
        return some(7)
    }
    g := fn() {
        return
    }
    g()
    match f(false) {
        some(v) => { return int(v) }
        none => { return 0 }
    }
}
"#,
    );
    assert!(
        go.contains("return option[uint8]{}"),
        "expected option[uint8]{{}} in closure return none, got:\n{}",
        go
    );
    assert!(
        !go.contains("return none"),
        "raw `return none` must not leak into Go:\n{}",
        go
    );
    assert!(
        !go.contains("return \n"),
        "bare return in closure must not emit trailing space:\n{}",
        go
    );

    // A non-option closure nested inside an option-returning outer function
    // must NOT inherit the outer function's option result type.
    let err = emit_err(
        r#"
fn outer_opt() option[int] {
    bad := fn() int {
        return none
    }
    _ = bad
    return none
}
"#,
    );
    assert!(
        err.contains("not option[T]"),
        "expected option[T] error for non-option closure, got: {}",
        err
    );
}

#[test]
fn fnlit_e3_try_blank_return_and_short_ok_return_use_closure_signature() {
    let go = emit(
        r#"
fn step(x int) (int, error) {
    return x + 1
}

fn outer() (string, error) {
    compute := fn(flag bool) (int, error) {
        v := step(10)?
        if !flag {
            return _, errors.new("bad flag")
        }
        return v
    }
    n := compute(true)?
    if n < 0 {
        return _, errors.new("neg")
    }
    return "ok"
}

fn main() {
    worker := fn() (int, error) {
        v := step(1)?
        return v
    }
    _ = worker
}
"#,
    );
    assert!(
        go.contains("return 0, hyErr0"),
        "closure `?` must propagate the closure's own int zero value (0), got:\n{}",
        go
    );
    assert!(
        go.contains("return 0, errors.New(\"bad flag\")"),
        "closure `return _, err` must fill `_` with 0, got:\n{}",
        go
    );
    assert!(
        go.contains("return v, nil"),
        "closure short ok-return `return v` must append `, nil`, got:\n{}",
        go
    );
    assert!(
        go.contains("return \"\", errors.New(\"neg\")") && go.contains("return \"ok\", nil"),
        "outer function must restore its own (string, error) return state after closure:\n{}",
        go
    );
    assert!(
        !go.contains("func() (hyErr error)"),
        "main with `?` only inside an inner closure must not wrap main in an unused hyErr closure:\n{}",
        go
    );
}

#[test]
fn n5_named_return_parameters_allocate_maps_and_naked_return_preserves_named_results() {
    let go = emit(
        r#"
struct bag {
    counts map[string]int
}

fn make_counts() (m map[string]u8) {
    m["a"] = 1
    return
}

fn make_bag() (b bag, err error) {
    b.counts["k"] = 2
    return
}

fn main() {
    build := fn() (m map[string]int) {
        m["x"] = 9
        return
    }
    fmt.println(make_counts()["a"], build()["x"])
}
"#,
    );
    assert!(
        go.contains("func make_counts() (m map[string]uint8) {\n\tm = map[string]uint8{}"),
        "expected N5 map allocation for named result `m`, got:\n{}",
        go
    );
    assert!(
        go.contains("func make_bag() (b bag, err error) {\n\tb = bag{counts: map[string]int{}}"),
        "expected N5 struct-map allocation for named result `b`, got:\n{}",
        go
    );
    assert!(
        !go.contains("return *new(bag), nil"),
        "naked return in `(b bag, err error)` must not overwrite named result `b` with zero:\n{}",
        go
    );
    assert!(
        go.contains("build := func() (m map[string]int) {\n\t\tm = map[string]int{}"),
        "expected N5 map allocation for closure named result `m`, got:\n{}",
        go
    );
}
