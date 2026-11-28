//! Curated package selectors contain both callables and Go values. Calling a
//! constant or package variable used to pass `hyb check` and emit invalid Go
//! such as `time.RFC3339()` or `io.EOF()`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("curated_package_values_not_callable.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn errors(src: &str) -> Vec<String> {
    check(src)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn curated_constants_and_variables_are_not_callable() {
    for expr in [
        "time.rfc3339()",
        "time.january()",
        "io.eof()",
        "io.discard()",
        "http.status_ok()",
        "http.method_get()",
        "http.default_client()",
        "net.default_resolver()",
        "strconv.int_size()",
        "errors.err_unsupported()",
        "bufio.max_scan_token_size()",
        "httptest.default_remote_addr()",
    ] {
        let src = format!("fn main() {{\n    fmt.println({expr})\n}}\n");
        assert!(
            errors(&src).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {expr}"
        );
        assert_eq!(
            check(&src).verdict,
            Verdict::Reject,
            "expected reject for {expr}"
        );
    }
}

#[test]
fn curated_values_without_call_parentheses_still_accept() {
    let src = r#"
fn main() {
    fmt.println(time.rfc3339)
    fmt.println(io.eof)
    fmt.println(http.status_ok)
    fmt.println(net.default_resolver)
    fmt.println(strconv.int_size)
}
"#;
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}

#[test]
fn shadowed_package_names_are_not_false_positive() {
    let src = r#"
struct holder {
    rfc3339 fn() string
}

fn main() {
    time := holder{rfc3339: fn() string { return "ok" }}
    fmt.println(time.rfc3339())
}
"#;
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}

#[test]
fn os_package_variables_keep_their_supported_zero_arg_lowering() {
    let src = r#"
fn main() {
    fmt.println(os.args())
    fmt.println(os.stdout())
}
"#;
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}
