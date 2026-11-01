//! #2901 — calls through fn VALUES reached by index, map read, field or call
//! result (`fs[0]("a")`, `m["a"](1)`, `v.cb("a")`, `mk()(1, 2)`) skipped the
//! arity, argument and result checks a direct call gets, so they passed
//! `hyb check` and failed `go build`.

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

#[test]
fn reject_slice_index_callee_arg_type() {
    assert_reject(r#"fn main() {
    fs := []fn(int) int{fn(x int) int { return x }}
    fmt.println(fs[0]("a"))
}
"#);
}

#[test]
fn reject_map_index_callee_result_type() {
    assert_reject(r#"fn main() {
    m := map[string]fn(int) int{"a": fn(x int) int { return x }}
    var s string = m["a"](1)
    fmt.println(s)
}
"#);
}

#[test]
fn reject_field_callee_arg_type() {
    assert_reject(r#"struct h {
    cb fn(int) int
}
fn main() {
    v := h{cb: fn(x int) int { return x }}
    fmt.println(v.cb("a"))
}
"#);
}

#[test]
fn reject_call_result_callee_arg_type() {
    assert_reject(r#"fn mk() fn(int) int { return fn(x int) int { return x } }
fn main() {
    fmt.println(mk()("a"))
}
"#);
}

#[test]
fn reject_call_result_callee_result_type() {
    assert_reject(r#"fn mk() fn(int) int { return fn(x int) int { return x } }
fn main() {
    var s string = mk()(1)
    fmt.println(s)
}
"#);
}

#[test]
fn reject_call_result_callee_arity() {
    assert_reject(r#"fn mk() fn(int) int { return fn(x int) int { return x } }
fn main() {
    fmt.println(mk()(1, 2))
}
"#);
}

#[test]
fn reject_call_result_callee_bound_local() {
    assert_reject(r#"fn mk() fn(int) int { return fn(x int) int { return x } }
fn main() {
    x := mk()(1)
    fmt.println(x + "a")
}
"#);
}

#[test]
fn reject_field_callee_arg_type_statement() {
    assert_reject(r#"struct h {
    cb fn(int) int
}
fn main() {
    v := h{cb: fn(x int) int { return x }}
    y := v.cb("a")
    fmt.println(y)
}
"#);
}

#[test]
fn accept_well_typed_fn_value_calls() {
    assert_accept(r#"struct h {
    cb fn(int, string) string
    g fn(a, b int) int
    f fn(x float64) float64
    p fn(any) string
}
fn mk() fn(int) int { return fn(x int) int { return x + 1 } }
fn pair() (int, string) { return 1, "a" }
fn main() {
    fs := []fn(int) int{fn(x int) int { return x * 2 }}
    m := map[string]fn(string) int{"a": fn(s string) int { return len(s) }}
    v := h{
        cb: fn(n int, s string) string { return s },
        g: fn(a int, b int) int { return a + b },
        f: fn(x float64) float64 { return x },
        p: fn(x any) string { return "p" },
    }
    var a int = fs[0](3)
    var b int = m["a"]("xyz")
    var c int = mk()(4)
    var d string = v.cb(1, "q")
    e := v.g(1, 2)
    var f float64 = v.f(2)
    g := v.p("s") + v.p(3)
    fmt.println(a, b, c, d, e, f, g)
}
"#);
}

#[test]
fn accept_grouped_and_type_only_fn_params() {
    assert_accept(r#"struct h {
    g fn(a, b int) string
    t fn(int, string) int
}
fn main() {
    v := h{g: fn(a int, b int) string { return "x" }, t: fn(n int, s string) int { return n }}
    fmt.println(v.g(1, 2), v.t(1, "a"))
}
"#);
}
