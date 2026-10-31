//! decl_and_multi_assign_types.rs — regressions for local declaration
//! initializer type mismatches, multi-assignment type mismatches, and
//! declaration/assignment arity mismatches.
//!
//! Go compiler guarantees:
//!   1. `var x T = v` / `let x T = v` / `const x T = v`: type of `v` must match `T`.
//!   2. `x, y = v1, v2`: types of `v1` and `v2` must match `x` and `y`.
//!   3. Arity mismatch: `assignment mismatch: N variables but M values`.
//!
//! All reject as HYB-E1002 (rule `BUILDABLE`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(r: &hy_check::CheckResult, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

fn assert_accepts(r: &hy_check::CheckResult, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.is_empty(),
        "expected accept for {}, got errors: {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Accept, "{}", what);
}

#[test]
fn rejects_var_decl_type_mismatch() {
    let src = r#"
package main
import "fmt"
fn main() {
    var x int = "hello"
    fmt.println(x)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "var x int = \"hello\"");
}

#[test]
fn rejects_let_decl_type_mismatch() {
    let src = r#"
package main
import "fmt"
fn main() {
    let b bool = 123
    fmt.println(b)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "let b bool = 123");
}

#[test]
fn rejects_const_decl_type_mismatch() {
    let src = r#"
package main
import "fmt"
fn main() {
    const s string = 456
    fmt.println(s)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "const s string = 456");
}

#[test]
fn rejects_multi_assign_type_swap_mismatch() {
    let src = r#"
package main
import "fmt"
fn main() {
    x := 10
    s := "hello"
    x, s = s, x
    fmt.println(x, s)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "x, s = s, x");
}

#[test]
fn rejects_multi_assign_arity_more_values() {
    let src = r#"
package main
import "fmt"
fn main() {
    x := 1
    y := 2
    x, y = 10, 20, 30
    fmt.println(x, y)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "x, y = 10, 20, 30");
}

#[test]
fn rejects_multi_assign_arity_fewer_values() {
    let src = r#"
package main
import "fmt"
fn main() {
    x := 1
    y := 2
    z := 3
    x, y, z = 10, 20
    fmt.println(x, y, z)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "x, y, z = 10, 20");
}

#[test]
fn rejects_short_decl_arity_mismatch() {
    let src = r#"
package main
import "fmt"
fn main() {
    a, b := 1, 2, 3
    fmt.println(a, b)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "a, b := 1, 2, 3");
}

#[test]
fn rejects_short_decl_single_scalar_to_multiple_vars() {
    let src = r#"
package main
import "fmt"
fn main() {
    a, b := 10
    fmt.println(a, b)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "a, b := 10");
}

#[test]
fn rejects_assign_single_scalar_to_multiple_vars() {
    let src = r#"
package main
import "fmt"
fn main() {
    a := 1
    b := 2
    a, b = 10
    fmt.println(a, b)
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "a, b = 10");
}

#[test]
fn accepts_matching_types_and_untyped_constants() {
    let src = r#"
package main
import "fmt"
fn two() (int, string) {
    return 1, "ok"
}
fn main() {
    var x int = 10
    var f float64 = 1
    a, b := 1, 2
    a, b = b, a
    p, q := two()
    fmt.println(x, f, a, b, p, q)
}
"#;
    let r = check(src);
    assert_accepts(&r, "valid initializers, assignments, untyped constants, and calls");
}
