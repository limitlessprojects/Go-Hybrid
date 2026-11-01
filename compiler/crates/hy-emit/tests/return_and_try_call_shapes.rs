//! Regression tests for `emit_return` (multi-value `(T, error)` call forwarding,
//! naked `return` in named-result `error` functions/closures, and closure
//! return lowering) and `call_results` (`?` arity resolution across user
//! methods, associated functions, inline/aliased/param closures, and chained
//! foreign handle receivers).

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn return_forwards_multi_value_error_calls_and_pads_single_value_calls() {
    let go = emit(
        "struct worker { base int }\nfn (w *worker) load() (int, error) { return w.base, nil }\nfn step() (int, error) { return 7, nil }\nfn pure_val() int { return 9 }\nfn forward_fn() (int, error) { return step() }\nfn forward_paren() (int, error) { return (step()) }\nfn forward_method(w *worker) (int, error) { return w.load() }\nfn pad_pure() (int, error) { return pure_val() }\nfn main() {\n    f := fn() (int, error) { return step() }\n    _, _ = f()\n}\n",
    );
    assert!(go.contains("func forward_fn() (int, error) {\n\treturn step()\n}"), "{go}");
    assert!(go.contains("func forward_paren() (int, error) {\n\treturn (step())\n}"), "{go}");
    assert!(go.contains("func forward_method(w *worker) (int, error) {\n\treturn w.load()\n}"), "{go}");
    assert!(go.contains("func pad_pure() (int, error) {\n\treturn pure_val(), nil\n}"), "{go}");
    assert!(go.contains("f := func() (int, error) {\n\t\treturn step()\n\t}"), "{go}");
}

#[test]
fn naked_return_in_named_error_fn_and_closure_does_not_zero_overwrite() {
    let go = emit(
        "fn compute() (out int, err error) {\n    out = 42\n    return\n}\nfn main() {\n    f := fn() (v int, err error) {\n        v = 7\n        return\n    }\n    _, _ = compute()\n    _, _ = f()\n}\n",
    );
    assert!(go.contains("out = 42\n\treturn\n}"), "{go}");
    assert!(!go.contains("return 0, nil"), "{go}");
    assert!(go.contains("v = 7\n\t\treturn\n\t}"), "{go}");
}

#[test]
fn closure_and_named_result_return_lowering_for_e3_option_and_handler() {
    let go = emit(
        "fn find_named() (out option[int]) {\n    return none\n}\nfn main() {\n    e3 := fn(err error) (int, error) {\n        if err != nil { return _, err }\n        return 42\n    }\n    opt := fn() option[int] { return none }\n    h := fn() http.handler {\n        return fn(w, r) { fmt.fprintln(w, \"ok\") }\n    }\n    _, _ = e3(nil)\n    _ = opt()\n    _ = h()\n    _ = find_named()\n}\n",
    );
    assert!(go.contains("func find_named() (out option[int]) {\n\treturn option[int]{}\n}"), "{go}");
    assert!(go.contains("return 0, err"), "{go}");
    assert!(go.contains("return 42, nil"), "{go}");
    assert!(go.contains("opt := func() (option[int]) {\n\t\treturn option[int]{}\n\t}"), "{go}");
    assert!(go.contains("return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request)"), "{go}");
}

#[test]
fn try_bare_error_arity_across_methods_assoc_closures_and_handles() {
    let go = emit(
        "struct worker { hook fn() error }\nfn worker.ping() error { return nil }\nfn (w *worker) step() error { return nil }\nfn make_worker() *worker { return &worker{hook: fn() error { return nil }} }\nfn run(cb fn() error) error {\n    w := make_worker()\n    w.step()?\n    w.hook()?\n    worker.ping()?\n    cb()?\n    alias := w.step\n    alias()?\n    (fn() error { return nil })()?\n    (worker.ping())?\n    doc := serde_json.new_object()\n    (doc).set(\"k\", 1)?\n    return nil\n}\nfn main() { _ = run(fn() error { return nil }) }\n",
    );
    for expected in [
        "hyErr0 := w.step()",
        "hyErr1 := w.hook()",
        "hyErr2 := worker_ping()",
        "hyErr3 := cb()",
        "hyErr4 := alias()",
        "hyErr5 := (func() (error) {\n\t\treturn nil\n\t})()",
        "hyErr6 := (worker_ping())",
        "hyErr7 := doc.Set(\"k\", 1)",
    ] {
        assert!(go.contains(expected), "missing `{expected}` in:\n{go}");
    }
}
