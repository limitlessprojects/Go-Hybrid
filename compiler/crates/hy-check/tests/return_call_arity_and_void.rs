//! Return call arity and void call guards (HYB-E1002).
//!
//! Validates that a return statement carrying a single function call expression
//! (`return f()`) supplies exactly the results declared by the enclosing
//! signature, preventing void calls and arity mismatches from bypassing
//! type checking into invalid emitted Go.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_clean(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn rejects_multi_value_call_in_single_result_fn() {
    assert_code(
        r#"
package main
import "fmt"
fn f() (int, int) {
    return 1, 2
}
fn g() int {
    return f()
}
fn main() {
    fmt.println(g())
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_void_call_in_single_result_fn() {
    assert_code(
        r#"
package main
import "fmt"
fn f() {}
fn g() int {
    return f()
}
fn main() {
    fmt.println(g())
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_three_results_call_in_two_results_fn() {
    assert_code(
        r#"
package main
import "fmt"
fn f() (int, int, int) {
    return 1, 2, 3
}
fn g() (int, int) {
    return f()
}
fn main() {
    a, b := g()
    fmt.println(a, b)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_one_result_call_in_two_results_fn_without_error_tail() {
    assert_code(
        r#"
package main
import "fmt"
fn f() int {
    return 1
}
fn g() (int, int) {
    return f()
}
fn main() {
    a, b := g()
    fmt.println(a, b)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_value_call_in_void_fn() {
    assert_code(
        r#"
package main
import "fmt"
fn f() int {
    return 1
}
fn g() {
    return f()
}
fn main() {
    g()
    fmt.println("ok")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_method_call_multi_result_in_single_result_fn() {
    assert_code(
        r#"
package main
import "fmt"
struct calc {}
fn (c calc) pair() (int, int) {
    return 1, 2
}
fn single() int {
    c := calc{}
    return c.pair()
}
fn main() {
    fmt.println(single())
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_builtin_delete_in_value_return() {
    assert_code(
        r#"
package main
import "fmt"
fn g(m map[string]int) int {
    return delete(m, "k")
}
fn main() {
    m := map[string]int{}
    fmt.println(g(m))
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_builtin_close_in_value_return() {
    assert_code(
        r#"
package main
import "fmt"
fn g(ch chan int) int {
    return close(ch)
}
fn main() {
    ch := make(chan int)
    fmt.println(g(ch))
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_closure_multi_result_in_single_result_fn() {
    assert_code(
        r#"
package main
import "fmt"
fn single() int {
    f := fn() (int, int) {
        return 1, 2
    }
    return f()
}
fn main() {
    fmt.println(single())
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_void_call_in_error_tail_fn() {
    assert_code(
        r#"
package main
import "fmt"
fn f() {}
fn g() (int, error) {
    return f()
}
fn main() {
    v, _ := g()
    fmt.println(v)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn rejects_excess_results_call_in_error_tail_fn() {
    assert_code(
        r#"
package main
import "fmt"
fn f() (int, int, int) {
    return 1, 2, 3
}
fn g() (int, error) {
    return f()
}
fn main() {
    v, _ := g()
    fmt.println(v)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn accepts_matching_multi_result_call() {
    assert_clean(
        r#"
package main
import "fmt"
fn f() (int, int) {
    return 1, 2
}
fn g() (int, int) {
    return f()
}
fn main() {
    a, b := g()
    fmt.println(a, b)
}
"#,
    );
}

#[test]
fn accepts_void_call_in_void_fn() {
    assert_clean(
        r#"
package main
fn f() {}
fn g() {
    return f()
}
fn main() {
    g()
}
"#,
    );
}

#[test]
fn accepts_error_tail_zero_fill_call() {
    assert_clean(
        r#"
package main
import "fmt"
fn f() int {
    return 42
}
fn g() (int, error) {
    return f()
}
fn main() {
    v, _ := g()
    fmt.println(v)
}
"#,
    );
}
