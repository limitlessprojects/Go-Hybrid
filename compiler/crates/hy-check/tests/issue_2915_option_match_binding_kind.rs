//! #2915 — the payload binding of an `option[T]` match arm (`some(v)`) was
//! bound as `OpKind::Unknown`, so var-init, argument, return and arithmetic
//! checks all skipped it; and `option[T].or(x)` never checked `x` against `T`.
//! Each reject case passed `hyb check` and failed `go build`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

const FIND: &str = "fn find() option[int] { return some(1) }\n";

fn with_find(body: &str) -> String {
    format!("{FIND}{body}")
}

#[test]
fn reject_some_binding_var_init() {
    assert_reject(&with_find(r#"fn main() {
    match find() {
        some(v): {
            var s string = v
            fmt.println(s)
        }
        none: fmt.println("n")
    }
}
"#));
}

#[test]
fn reject_some_binding_arith() {
    assert_reject(&with_find(r#"fn main() {
    match find() {
        some(v): fmt.println(v + "a")
        none: fmt.println("n")
    }
}
"#));
}

#[test]
fn reject_some_binding_argument() {
    assert_reject(&with_find(r#"fn take(s string) { fmt.println(s) }
fn main() {
    match find() {
        some(v): take(v)
        none: fmt.println("n")
    }
}
"#));
}

#[test]
fn reject_some_binding_return() {
    assert_reject(&with_find(r#"fn get() string {
    match find() {
        some(v): return v
        none: return "n"
    }
}
fn main() { fmt.println(get()) }
"#));
}

#[test]
fn reject_some_binding_slice_elem() {
    assert_reject(r#"fn find() option[[]int] { return some([]int{1}) }
fn main() {
    match find() {
        some(xs): {
            var s string = xs[0]
            fmt.println(s)
        }
        none: fmt.println("n")
    }
}
"#);
}

#[test]
fn reject_some_binding_from_local_option() {
    assert_reject(r#"fn main() {
    var o option[string] = some("a")
    match o {
        some(v): fmt.println(v + 1)
        none: fmt.println("n")
    }
}
"#);
}

#[test]
fn reject_or_default_wrong_type() {
    assert_reject(&with_find(r#"fn main() {
    x := find().or("a")
    fmt.println(x)
}
"#));
}

#[test]
fn accept_well_typed_option_uses() {
    assert_accept(r#"fn find() option[int] { return some(1) }
fn findf() option[float64] { return some(1.5) }
fn names() option[[]string] { return some([]string{"a"}) }
fn take(n int) int { return n }
fn get() int {
    match find() {
        some(v): return v + 1
        none: return 0
    }
}
fn main() {
    match find() {
        some(v): {
            var n int = v
            fmt.println(take(v) + n)
        }
        none: fmt.println("n")
    }
    match names() {
        some(xs): {
            var s string = xs[0]
            fmt.println(s + "!")
        }
        none: fmt.println("n")
    }
    x := find().or(7)
    y := findf().or(2)
    z := find().or(take(3))
    fmt.println(x, y, z, get())
}
"#);
}
