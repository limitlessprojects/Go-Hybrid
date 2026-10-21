//! Regression tests for `HYB-E2002` (T23: a multi-value result is not a tuple):
//! 1. Channel receive (`<-ch`) yields at most 2 values (`v` and optional `ok`),
//!    so binding 3 or more variables (`a, b, c := <-ch` or `a, b, c = <-ch`)
//!    must reject with `HYB-E2002` instead of passing front-end check and failing `go build`.
//! 2. Map index (`m[k]`) yields at most 2 values (`v` and optional `ok`),
//!    so binding 3 or more variables (`a, b, c := m[k]` or `a, b, c = m[k]`)
//!    must reject with `HYB-E2002`.
//! 3. Slice, array, and string indexing (`xs[i]`) yields exactly 1 value (no comma-ok),
//!    so binding 2 or more variables (`a, b := xs[i]` or `a, b = xs[i]`)
//!    must reject with `HYB-E2002`.
//! 4. Select receive statements (`select { case a, b, c := <-ch: }`) yield at most
//!    2 values, so binding 3 or more variables in top-level or closure selects
//!    must reject with `HYB-E2002` instead of crashing `hy-emit` / failing `go build`.
//! 5. Legal 1-value and 2-value forms remain accepted with 0 diagnostics.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("must parse");
    check_file(&f)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn channel_receive_three_or_more_variables_define_rejects_e2002() {
    let src = r#"
fn main() {
    ch := make(chan int, 1)
    a, b, c := <-ch
    _ = a
    _ = b
    _ = c
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn channel_receive_three_or_more_variables_assign_rejects_e2002() {
    let src = r#"
fn main() {
    ch := make(chan int, 1)
    var a int
    var b bool
    var c bool
    a, b, c = <-ch
    _ = a
    _ = b
    _ = c
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn map_index_three_or_more_variables_define_rejects_e2002() {
    let src = r#"
fn main() {
    m := map[string]int{"k": 1}
    a, b, c := m["k"]
    _ = a
    _ = b
    _ = c
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn map_index_three_or_more_variables_assign_rejects_e2002() {
    let src = r#"
fn main() {
    m := map[string]int{"k": 1}
    var a int
    var b bool
    var c bool
    a, b, c = m["k"]
    _ = a
    _ = b
    _ = c
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn slice_index_two_or_more_variables_define_rejects_e2002() {
    let src = r#"
fn main() {
    xs := []int{1, 2}
    a, b := xs[0]
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn slice_index_two_or_more_variables_assign_rejects_e2002() {
    let src = r#"
fn main() {
    xs := []int{1, 2}
    var a int
    var b bool
    a, b = xs[0]
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn select_receive_three_or_more_variables_rejects_e2002() {
    let src = r#"
fn main() {
    ch := make(chan int, 1)
    select {
    a, b, c := <-ch:
        _ = a
        _ = b
        _ = c
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn select_receive_three_or_more_variables_in_closure_rejects_e2002() {
    let src = r#"
fn main() {
    f := fn() {
        ch := make(chan int, 1)
        select {
        a, b, c := <-ch:
            _ = a
            _ = b
            _ = c
        }
    }
    f()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2002".to_string()));
}

#[test]
fn valid_channel_receive_map_index_and_select_receive_accepted() {
    let src = r#"
fn main() {
    ch := make(chan int, 2)
    ch <- 1
    ch <- 2

    // 1-value and 2-value channel receives
    v1 := <-ch
    _ = v1
    v2, ok2 := <-ch
    _ = v2
    _ = ok2

    // 1-value and 2-value map index
    m := map[string]int{"k": 10}
    mv1 := m["k"]
    _ = mv1
    mv2, mok2 := m["k"]
    _ = mv2
    _ = mok2

    // 1-value slice index
    xs := []int{100, 200}
    sv := xs[0]
    _ = sv

    // Select with 0, 1, and 2 bindings
    select {
    <-ch:
        fmt.println("zero")
    s1 := <-ch:
        fmt.println(s1)
    s2, sok := <-ch:
        _ = s2
        _ = sok
    default:
        fmt.println("default")
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty(), "unexpected errors: {:?}", r.diagnostics);
}
