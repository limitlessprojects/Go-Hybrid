use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("discard_const_read.hy", src).expect("must parse");
    check_file(&ast)
}

/// Issue #460: `_` is a discard, not a place. Two consecutive discards where the first
/// reads a `const` composite must not be reported as a write to that const. Before the
/// guard in `const_write_root`, the second `_ = ...` was treated as a rebind of `_`,
/// whose alias walk reached the const's storage and emitted HYB-E3006 naming a write
/// the program does not contain.
#[test]
fn two_discards_after_a_const_read_are_accepted() {
    let src = r#"
const shared = map[string]int{"k": 1}

fn main() {
    const local = [1, 2]
    _ = shared["k"]
    _ = local[0]
}
"#;

    let res = check(src);
    assert!(
        matches!(res.verdict, Verdict::Accept),
        "reading a const twice through two discards is not a write: {:?}",
        res.diagnostics
    );
}

/// The same shape with a plain (non-const) map must stay accepted, so the fix is not
/// merely widening acceptance for const reads in general.
#[test]
fn two_discards_without_a_const_are_accepted() {
    let src = r#"
fn main() {
    m := map[string]int{"k": 1}
    xs := [1, 2]
    _ = m["k"]
    _ = xs[0]
}
"#;

    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Accept), "{:?}", res.diagnostics);
}

/// Guard against over-fixing: a real element write through a `const` root must still
/// be HYB-E3006, whether or not a discard read preceded it. This is the direction the
/// loose variant of the fix would break.
#[test]
fn a_real_const_write_still_rejects_after_a_discard_read() {
    let src = r#"
const shared = map[string]int{"k": 1}

fn main() {
    _ = shared["k"]
    shared["k"] = 2
}
"#;

    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject), "{:?}", res.diagnostics);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "a write through a const root is still T22: {:?}",
        res.diagnostics
    );
}

/// And the plain write with no discard involved must keep rejecting.
#[test]
fn a_plain_const_write_still_rejects() {
    let src = r#"
const shared = map[string]int{"k": 1}

fn main() {
    shared["k"] = 2
}
"#;

    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject), "{:?}", res.diagnostics);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "{:?}",
        res.diagnostics
    );
}
