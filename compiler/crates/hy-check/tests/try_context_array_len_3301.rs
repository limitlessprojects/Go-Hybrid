use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn try_in_array_length_inside_error_returning_and_main_contexts_rejected() {
    for src in [
        "fn val() (int, error) { return 2, nil }\nfn main() {\n    var a [val()?]int\n    _ = a\n}",
        "fn val() (int, error) { return 2, nil }\nfn f() (int, error) {\n    a := [val()?]int{1, 2}\n    return a[0], nil\n}\nfn main() {}",
        "fn val() (int, error) { return 2, nil }\nfn f() error {\n    _ = map[[val()?]int]int{}\n    return nil\n}\nfn main() {}",
        "fn val() (int, error) { return 2, nil }\nfn main() {\n    g := fn(x [val()?]int) error { _ = x\n return nil }\n    _ = g\n}",
        "import \"http\"\nfn val() (int, error) { return 2, nil }\nfn main() {\n    h := http.handler(fn(w, r) {\n        var buf [val()?]byte\n        _ = buf\n    })\n    _ = h\n}",
    ] {
        let r = check_file(&parse_file("t.hy", src).unwrap());
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E2011"),
            "expected HYB-E2011 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn const_array_length_with_try_in_value_expression_remains_accepted() {
    let src = "fn val() (int, error) { return 2, nil }\nfn main() {\n    a := [2]int{val()?, 0}\n    _ = a\n}";
    let r = check_file(&parse_file("t.hy", src).unwrap());
    assert!(r.diagnostics.is_empty(), "unexpected diagnostics: {:?}", r.diagnostics);
}
