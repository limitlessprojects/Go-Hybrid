//! set_results_and_methods_2921.rs — issue #2921 (Row 13 BUILDABLE, HYB-E1002).
//!
//! Regression test for issue #2921:
//! 1. `set[T]` methods `insert` and `contains` return `bool`, and `len` returns `int`.
//!    Assigning them to mismatched types (e.g. `string`) must be rejected with HYB-E1002.
//! 2. `for v in s` over `set[T]` binds `v` as `T`. Assigning `v` to mismatched types
//!    must be rejected with HYB-E1002.
//! 3. Indexed sets (e.g. `ss[0].insert("x")` where `ss := []set[int]{...}`) must be
//!    type-checked and rejected with HYB-E1002.
//! 4. Calling pointer methods on non-addressable sets (e.g. `m["a"].contains(1)`)
//!    must be rejected with HYB-E1002.
//! 5. Calling methods with wrong arity (`s.insert()`, `s.len(1)`), non-existent methods
//!    (`s.foo()`), or `.pop()` on a set must be rejected with HYB-E1002.
//! 6. Correctly-typed set operations stay accepted with 0 diagnostics.

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

#[test]
fn set_result_typing_mismatch_rejected() {
    let src = r#"
fn main() {
    s := set[int]{}
    var b string = s.insert(1)
    var c string = s.contains(1)
    var n string = s.len()
    _ = b
    _ = c
    _ = n
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 3,
        "expected at least three HYB-E1002 errors, got {codes:?}"
    );
}

#[test]
fn set_for_iterate_mismatch_rejected() {
    let src = r#"
import "fmt"

fn main() {
    s := set[int]{}
    s.insert(1)
    for v in s {
        var t string = v
        fmt.println(t)
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 error, got {codes:?}"
    );
}

#[test]
fn indexed_set_arg_mismatch_rejected() {
    let src = r#"
fn main() {
    ss := []set[int]{set[int]{}}
    ss[0].insert("x")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 error, got {codes:?}"
    );
}

#[test]
fn unaddressable_map_set_method_rejected() {
    let src = r#"
import "fmt"

fn main() {
    m := map[string]set[int]{}
    fmt.println(m["a"].contains(1))
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 error, got {codes:?}"
    );
}

#[test]
fn set_method_arity_and_pop_rejected() {
    let src = r#"
fn main() {
    s := set[int]{}
    s.pop()
    s.foo()
    s.insert()
    s.insert(1, 2)
    s.len(1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 5,
        "expected at least five HYB-E1002 errors, got {codes:?}"
    );
}

#[test]
fn set_valid_operations_stay_accepted() {
    let src = r#"
import "fmt"

fn main() {
    s := set[int]{}
    var b bool = s.insert(1)
    var c bool = s.contains(1)
    s.remove(1)
    var n int = s.len()
    if b && c && n > 0 {
        fmt.println("ok")
    }
    for v in s {
        _ = v + 1
    }
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid set operations must stay accepted: {:?}",
        r.diagnostics
    );
}
