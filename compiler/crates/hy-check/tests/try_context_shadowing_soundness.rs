//! Regression tests for E4 / E6 / E11 `?` propagation context soundness (`HYB-E2011`)
//! when `http`, the E11 writer parameter, `error`, E6 wrapper packages (`fmt`/`os`/`filepath`),
//! or explicit E11 parameter type aliases are shadowed at file or lexical scope.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn e2011(r: &CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E2011")
        .count()
}

const HELPERS: &str =
    "fn val() (int, error) {\n    return 1, nil\n}\n\nfn close_it() error {\n    return nil\n}\n\n";

fn with_helpers(body: &str) -> CheckResult {
    check(&format!("{HELPERS}{body}"))
}

#[test]
fn e11_handler_shadowed_http_or_writer_rejects_e2011() {
    for (label, src) in [
        (
            "writer param named http shadows package root",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(http, r) {\n        close_it()?\n    })\n}\n",
        ),
        (
            "request param named http shadows package root",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, http) {\n        close_it()?\n    })\n}\n",
        ),
        (
            "duplicate param names fn(w, w) shadow writer param",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, w) {\n        close_it()?\n    })\n}\n",
        ),
        (
            "local binding shadowing http before try in E11 closure",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, r) {\n        http := 1\n        _ = http\n        close_it()?\n    })\n}\n",
        ),
        (
            "direct try-define binding http := val()? in E11 closure",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, r) {\n        http := val()?\n        _ = http\n    })\n}\n",
        ),
        (
            "inner block shadowing writer param w before try",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, r) {\n        {\n            w := 1\n            _ = w\n            close_it()?\n        }\n    })\n}\n",
        ),
        (
            "direct try-define binding w := val()? in inner block",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, r) {\n        {\n            w := val()?\n            _ = w\n        }\n    })\n}\n",
        ),
        (
            "for loop variable shadowing writer param w in E11 return closure",
            "import \"net/http\"\n\nfn make_h() http.handler {\n    return fn(w, r) {\n        for w in 0..1 {\n            _ = w\n            close_it()?\n        }\n    }\n}\n\nfn main() {\n    _ = make_h()\n}\n",
        ),
        (
            "if-init binding shadowing writer param w in E11 closure",
            "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, r) {\n        if w := 1; w > 0 {\n            close_it()?\n        }\n    })\n}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn e11_handler_after_inner_shadow_scope_closes_still_accepts() {
    let r = with_helpers(
        "import \"net/http\"\n\nfn main() {\n    _ = http.handler(fn(w, r) {\n        {\n            w := 1\n            _ = w\n        }\n        close_it()?\n        fmt.fprintf(w, \"ok\")\n        _ = r\n    })\n}\n",
    );
    assert_eq!(e2011(&r), 0, "unshadowed after inner block must accept E11 `?`: {:?}", r.diagnostics);
}

#[test]
fn shadowed_or_cyclic_error_and_e6_wrapper_packages_reject_e2011() {
    for (label, src) in [
        (
            "outer local shadowing error before closure returning error",
            "fn main() {\n    error := 1\n    _ = error\n    f := fn() error {\n        close_it()?\n        return nil\n    }\n    _ = f\n}\n",
        ),
        (
            "outer param shadowing error before closure returning error",
            "fn run(error int) {\n    _ = error\n    f := fn() error {\n        close_it()?\n        return nil\n    }\n    _ = f\n}\n\nfn main() {\n    run(1)\n}\n",
        ),
        (
            "closure param named error shadowing closure result error",
            "fn main() {\n    f := fn(error int) error {\n        _ = error\n        close_it()?\n        return nil\n    }\n    _ = f\n}\n",
        ),
        (
            "fn param named error shadowing fn result error",
            "fn bad(error int) error {\n    _ = error\n    close_it()?\n    return nil\n}\n\nfn main() {\n    _ = bad(1)\n}\n",
        ),
        (
            "file-level const shadowing predeclared error in fn and main",
            "const error = 1\n\nfn close_it() int {\n    return 0\n}\n\nfn bad() (int, error) {\n    x := val()?\n    return x, 0\n}\n\nfn main() {\n    _ = error\n}\n",
        ),
        (
            "cyclic type alias type error = error in fn result",
            "type error = error\n\nfn bad() error {\n    close_it()?\n    return nil\n}\n\nfn main() {}\n",
        ),
        (
            "struct error shadowing E6 wrapper error in main",
            "struct error {}\n\nfn main() {\n    close_it()?\n}\n",
        ),
        (
            "file-level const os shadowing E6 wrapper package in main",
            "const os = 1\n\nfn main() {\n    _ = os\n    close_it()?\n}\n",
        ),
    ] {
        let r = if src.contains("const error = 1") {
            check(src)
        } else {
            with_helpers(src)
        };
        assert!(e2011(&r) >= 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn shadowed_e11_parameter_type_aliases_reject_e2011() {
    for (label, src) in [
        (
            "outer local shadowing writer type alias in E11 closure",
            "import \"net/http\"\n\ntype my_w = http.response_writer\n\nfn main() {\n    my_w := 1\n    _ = my_w\n    _ = http.handler(fn(w my_w, r *http.request) {\n        close_it()?\n    })\n}\n",
        ),
        (
            "outer local shadowing request type alias in E11 closure",
            "import \"net/http\"\n\ntype my_r = http.request\n\nfn main() {\n    my_r := 1\n    _ = my_r\n    _ = http.handler(fn(w http.response_writer, r *my_r) {\n        close_it()?\n    })\n}\n",
        ),
        (
            "struct shadowing writer type alias in E11 closure",
            "import \"net/http\"\n\ntype my_w = http.response_writer\nstruct my_w {}\n\nfn main() {\n    _ = http.handler(fn(w my_w, r *http.request) {\n        close_it()?\n    })\n}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}
