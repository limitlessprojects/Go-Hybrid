fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn aliased_error_fn_try_propagation_emits_go() {
    let src = r#"type failure = error

fn val() (int, error) {
    return 1, nil
}

fn outer() (int, failure) {
    x := val()?
    return x, nil
}

fn main() {
    _, _ = outer()
}
"#;
    let go = emit(src);
    assert!(go.contains("type failure = error"), "{go}");
    assert!(go.contains("return 0, hyErr"), "{go}");
}

#[test]
fn chained_aliased_error_bare_call_try_emits_single_target() {
    let src = r#"type e1 = error
type e2 = e1

fn step() e2 {
    return nil
}

fn outer() e2 {
    step()?
    return nil
}

fn main() {
    step()?
}
"#;
    let go = emit(src);
    assert!(go.contains("hyErr0 := step()"), "{go}");
    assert!(!go.contains(", hyErr0 := step()"), "{go}");
    assert!(!go.contains(", hyErr1 := step()"), "{go}");
}

#[test]
fn aliased_error_closure_try_and_bare_call_emit_go() {
    let src = r#"type failure = error

fn val() (int, error) {
    return 1, nil
}

fn main() {
    f := fn() failure {
        return nil
    }
    f()?

    g := fn() (int, failure) {
        x := val()?
        return x, nil
    }
    _, _ = g()
}
"#;
    let go = emit(src);
    assert!(go.contains(":= f()"), "{go}");
    assert!(!go.contains(", hyErr0 := f()"), "{go}");
    assert!(!go.contains(", hyErr1 := f()"), "{go}");
    assert!(go.contains("return 0, hyErr"), "{go}");
}

#[test]
fn aliased_error_short_return_and_underscore_zero_fill() {
    let src = r#"type failure = error

fn outer(ok bool, err error) (int, failure) {
    if ok {
        return 7
    }
    return _, err
}
"#;
    let go = emit(src);
    assert!(go.contains("return 7, nil"), "{go}");
    assert!(go.contains("return 0, err"), "{go}");
}

#[test]
fn type_param_or_struct_shadowing_error_not_misclassified() {
    let src_tp = r#"type failure = error

fn wrap[failure any](v failure) failure {
    return v
}

fn main() {
    _ = wrap[int](1)
}
"#;
    let go_tp = emit(src_tp);
    assert!(
        go_tp.contains("func wrap[failure any](v failure) failure"),
        "{go_tp}"
    );

    let src_struct = r#"struct error {
    msg string
}

fn make_err() error {
    return error{msg: "x"}
}

fn main() {
    _ = make_err()
}
"#;
    let go_struct = emit(src_struct);
    assert!(go_struct.contains("func make_err() error"), "{go_struct}");
}
