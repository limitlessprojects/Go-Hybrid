//! Regression pin for #3257: a read of a loaned slot through its container
//! inside the live O8 window observes the stale pre-loan value.
//!
//! The loan lowers to copy-out now / store-back when its window ends, so any
//! container-relative read in between hits the old value while the program
//! is accepted with 0 diagnostics. `emit_o8_alias_if_live` already rejects
//! writes; these tests pin the read side.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("stale_slot_read_3257.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

/// The report's first program: a map-slot read while the loan is live.
#[test]
fn map_slot_read_inside_window_is_rejected() {
    let got = error_codes(
        r#"struct cell { n int }

fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    fmt.println(m["a"].n)
    p.n = 8
    fmt.println(m["a"].n)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102, got {got:?}"
    );
}

/// The slice-element variant, read into a local rather than printed.
#[test]
fn slice_slot_read_inside_window_is_rejected() {
    let got = error_codes(
        r#"struct cell { n int }

fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    p.n = 7
    v := xs[0].n
    p.n = 8
    fmt.println(v)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102, got {got:?}"
    );
}

/// Negative control: reading through the loan binding is the intended path.
#[test]
fn read_through_the_loan_binding_stays_accepted() {
    let got = error_codes(
        r#"struct cell { n int }

fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    p := &m["a"]
    p.n = 7
    fmt.println(p.n)
    p.n = 8
    fmt.println(p.n)
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: a container read BEFORE the loan is not stale.
#[test]
fn container_read_before_the_loan_stays_accepted() {
    let got = error_codes(
        r#"struct cell { n int }

fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    fmt.println(m["a"].n)
    p := &m["a"]
    p.n = 9
    fmt.println(p.n)
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: a different slot key is a different slot.
#[test]
fn read_of_a_different_slot_stays_accepted() {
    let got = error_codes(
        r#"struct cell { n int }

fn main() {
    m := map[string]cell{"a": cell{n: 1}, "b": cell{n: 2}}
    p := &m["a"]
    p.n = 9
    fmt.println(m["b"].n)
    p.n = 10
    fmt.println(p.n)
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: a read-only helper that only calls len() must stay
/// accepted. This is the case that pinned the scope down: the check rejects
/// index reads of the slot, not every hand-off of the container.
#[test]
fn read_only_helper_in_window_stays_accepted() {
    let got = error_codes(
        r#"struct cell { n int }

fn count(m map[string]cell) int {
    return len(m)
}

fn main() {
    m := map[string]cell{"a": cell{n: 0}}
    e := &m["a"]
    n := count(m)
    e.n = n
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}