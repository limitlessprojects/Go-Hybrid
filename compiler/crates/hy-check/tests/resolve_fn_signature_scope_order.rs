//! Regression tests for `resolve.rs` (`check_file_undefined_names` and `Expr::FnLit`):
//! function and closure receiver, parameter, and named result bindings enter scope
//! ONLY inside the function body, never inside signature type expressions
//! (`fn foo(n int, a [n + 1]int)`, `fn foo(n int) [n + 1]int`,
//! `f := fn(n int, a [n + 1]int) {}`) (`HYB-E1001`, GUARANTEES Row 13 `BUILDABLE`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parses");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn rejects_fn_and_closure_params_referenced_in_signature_array_lengths() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn foo(n int, a [n + 1]int) {
    fmt.println(n, len(a))
}
fn bar(n int) [n + 1]int {
    fmt.println(n)
    return [2]int{1, 2}
}
fn main() {
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E1001", "HYB-E1001"],
        "parameter `n` referenced inside `[n + 1]int` in parameter or result type must reject HYB-E1001"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    f := fn(n int, a [n + 1]int) {
        fmt.println(n, len(a))
    }
    _ = f
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E1001"],
        "closure parameter `n` referenced inside `[n + 1]int` in closure signature must reject HYB-E1001"
    );
}

#[test]
fn accepts_outer_consts_and_body_uses_of_fn_and_closure_params() {
    assert!(
        codes(
            r#"import "fmt"
const k = 2
fn foo(n int, a [k + 1]int) {
    fmt.println(n, len(a))
}
fn main() {
    var a [3]int
    foo(1, a)
}
"#,
        )
        .is_empty(),
        "outer file-level `const k` in signature array length and body use of parameter `n` must pass"
    );
}
