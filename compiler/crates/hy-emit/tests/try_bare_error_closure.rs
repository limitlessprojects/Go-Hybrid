//! `?` on a closure bound by name that returns bare `error`
//! (`check := fn() error {…}; check()?`) passed `hyb check` and emitted
//! `hyT0, hyErr1 := check()` — `go build`: "assignment mismatch: 2
//! variables but check returns 1 value". #2246 fixed the same arity slip
//! for top-level fns (`fn_shapes`); a closure name is not a top-level fn,
//! so it fell through to the "unknown = one value + error" default.

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn bare_error_closure_value_under_try_has_no_value_temp() {
    let go = emit(
        "fn main() {\n    check := fn() error {\n        return nil\n    }\n    check()?\n    fmt.println(\"ok\")\n}\n",
    );
    assert!(go.contains("hyErr0 := check()"), "{go}");
    assert!(!go.contains(", hyErr0 := check()"), "{go}");
}

#[test]
fn bare_error_closure_value_in_error_fn() {
    let go = emit(
        "fn run() error {\n    step := fn() error {\n        return nil\n    }\n    step()?\n    return nil\n}\n\nfn main() {\n    _ = run()\n}\n",
    );
    assert!(go.contains("hyErr0 := step()"), "{go}");
    assert!(!go.contains(", hyErr0 := step()"), "{go}");
}

#[test]
fn value_error_closure_keeps_its_value_temp() {
    let go = emit(
        "fn main() {\n    get := fn() (int, error) {\n        return 4, nil\n    }\n    v := get()?\n    fmt.println(v)\n}\n",
    );
    assert!(go.contains("v, hyErr0 := get()"), "{go}");
}

#[test]
fn rebinding_a_closure_name_to_a_non_closure_forgets_its_shape() {
    // `check` first names a bare-error closure, then (shadowed in a block)
    // a (bool, error) top-level fn value — the stale bare-error shape must
    // not leak to the second binding.
    let go = emit(
        "fn pair() (bool, error) {\n    return true, nil\n}\n\nfn main() {\n    check := fn() error {\n        return nil\n    }\n    check()?\n    if true {\n        check := pair\n        ok := check()?\n        fmt.println(ok)\n    }\n}\n",
    );
    assert!(go.contains("hyErr0 := check()"), "{go}");
    assert!(go.contains("ok, hyErr1 := check()"), "{go}");
}
