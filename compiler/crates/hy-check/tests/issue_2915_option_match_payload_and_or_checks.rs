//! issue_2915_option_match_payload_and_or_checks.rs — regression tests for Issue #2915:
//! option match payload binding `some(v)` is typed, and `option[T].or(x)` argument
//! is checked against `T` (HYB-E1002) under Row 13 (BUILDABLE).

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

fn assert_reject(name: &str, src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}: {:?}", name, r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for {}, got: {:?}",
        name,
        r.diagnostics
    );
}

fn assert_accept(name: &str, src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept for {}: {:?}", name, r.diagnostics);
    assert!(error_codes(&r).is_empty(), "expected no errors for {}, got: {:?}", name, r.diagnostics);
}

#[test]
fn test_reproducer_row1_match_var_decl_mismatch() {
    assert_reject(
        "Row 1: var s string = v in some(v) match arm",
        r#"
import "fmt"
fn find() option[int] { return some(1) }
fn main() {
    match find() {
        some(v): {
            var s string = v
            fmt.println(s)
        }
        none: fmt.println("n")
    }
}
"#,
    );
}

#[test]
fn test_reproducer_row2_match_binary_arith_mismatch() {
    assert_reject(
        "Row 2: v + \"a\" in some(v) match arm",
        r#"
import "fmt"
fn find() option[int] { return some(1) }
fn main() {
    match find() {
        some(v): fmt.println(v + "a")
        none: fmt.println("n")
    }
}
"#,
    );
}

#[test]
fn test_reproducer_row3_match_fn_arg_mismatch() {
    assert_reject(
        "Row 3: take(v) with fn take(s string)",
        r#"
import "fmt"
fn find() option[int] { return some(1) }
fn take(s string) { _ = s }
fn main() {
    match find() {
        some(v): take(v)
        none: fmt.println("n")
    }
}
"#,
    );
}

#[test]
fn test_reproducer_row4_match_return_type_mismatch() {
    assert_reject(
        "Row 4: fn get() string returning v",
        r#"
fn find() option[int] { return some(1) }
fn get() string {
    match find() {
        some(v): return v
        none: return "n"
    }
}
fn main() {
    _ = get()
}
"#,
    );
}

#[test]
fn test_reproducer_row5_match_slice_elem_var_decl_mismatch() {
    assert_reject(
        "Row 5: var s string = xs[0] for option[[]int]",
        r#"
import "fmt"
fn find() option[[]int] { return some([]int{1}) }
fn main() {
    match find() {
        some(xs): {
            var s string = xs[0]
            fmt.println(s)
        }
        none: fmt.println("n")
    }
}
"#,
    );
}

#[test]
fn test_reproducer_row6_option_or_arg_type_mismatch() {
    assert_reject(
        "Row 6: find().or(\"a\") with option[int]",
        r#"
fn find() option[int] { return some(1) }
fn main() {
    x := find().or("a")
    _ = x
}
"#,
    );
}

#[test]
fn test_option_or_arity_and_spread_rejected() {
    assert_reject(
        "or with 0 arguments",
        r#"
fn find() option[int] { return some(1) }
fn main() {
    _ = find().or()
}
"#,
    );
    assert_reject(
        "or with 2 arguments",
        r#"
fn find() option[int] { return some(1) }
fn main() {
    _ = find().or(1, 2)
}
"#,
    );
}

#[test]
fn test_struct_field_option_match_rejected() {
    assert_reject(
        "struct field option match payload mismatch",
        r#"
struct holder {
    opt option[int]
}
fn main() {
    h := holder{opt: some(10)}
    match h.opt {
        some(v): {
            var s string = v
            _ = s
        }
        none: {}
    }
}
"#,
    );
}

#[test]
fn test_valid_option_match_and_or_accepted() {
    assert_accept(
        "valid option match with matching types and valid .or",
        r#"
fn find() option[int] { return some(1) }
fn get() int {
    match find() {
        some(v): return v + 10
        none: return 0
    }
}
fn main() {
    match find() {
        some(v): {
            var n int = v
            _ = n + 5
        }
        none: {}
    }
    _ = find().or(42)
}
"#,
    );
}
