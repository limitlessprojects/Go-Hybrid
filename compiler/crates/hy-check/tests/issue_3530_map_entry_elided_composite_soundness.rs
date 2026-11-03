//! issue_3530_map_entry_elided_composite_soundness.rs — regression tests for issue #3530 (Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check` must emit Go code
//! that successfully compiles under `go build`.
//!
//! In `emit_guard.rs`, `check_expr_emit_guard` for `Expr::MapLit` validated duplicate keys and checked
//! `check_const_overflow_init` on top-level key and value expressions, but omitted array length bounds
//! (`HYB-E6004`) and element const-overflow (`HYB-E6002`) checks on elided array/slice entries
//! (`Expr::SliceLit { elem: None, .. }`).
//!
//! Consequently, map literals containing array entries exceeding declared length or overflowing scalar
//! elements passed `hyb check` with 0 diagnostics and failed closed during `go build`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject_with_code(src: &str, expected_code: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == expected_code),
        "expected {expected_code} for {what}, got {cs:?}"
    );
}

#[test]
fn rejects_map_value_array_len_exceeded() {
    let src = r#"
package main

fn main() {
    x := map[string][2]int{"a": {1, 2, 3}}
    _ = x
}
"#;
    assert_reject_with_code(src, "HYB-E6004", "map value array literal exceeding declared length");
}

#[test]
fn rejects_map_key_array_len_exceeded() {
    let src = r#"
package main

fn main() {
    x := map[[2]int]string{{1, 2, 3}: "a"}
    _ = x
}
"#;
    assert_reject_with_code(src, "HYB-E6004", "map key array literal exceeding declared length");
}

#[test]
fn rejects_nested_map_array_len_exceeded() {
    let src = r#"
package main

fn main() {
    x := map[string][2][2]int{"a": {{1, 2}, {3, 4, 5}}}
    _ = x
}
"#;
    assert_reject_with_code(src, "HYB-E6004", "nested map value array literal exceeding length");
}

#[test]
fn rejects_map_array_value_const_overflow() {
    let src = r#"
package main

fn main() {
    x := map[string][2]uint8{"a": {300, 1}}
    _ = x
}
"#;
    assert_reject_with_code(src, "HYB-E6002", "map array value element constant overflow");
}

#[test]
fn rejects_map_slice_value_const_overflow() {
    let src = r#"
package main

fn main() {
    x := map[string][]uint8{"a": {300, 1}}
    _ = x
}
"#;
    assert_reject_with_code(src, "HYB-E6002", "map slice value element constant overflow");
}

#[test]
fn rejects_map_array_key_const_overflow() {
    let src = r#"
package main

fn main() {
    x := map[[2]uint8]string{{300, 1}: "a"}
    _ = x
}
"#;
    assert_reject_with_code(src, "HYB-E6002", "map array key element constant overflow");
}

#[test]
fn accepts_valid_map_composite_entries() {
    let src = r#"
package main

fn main() {
    x := map[string][2]int{"a": {1, 2}}
    y := map[[2]int]string{{1, 2}: "a"}
    z := map[string][]uint8{"a": {255, 0}}
    _ = x
    _ = y
    _ = z
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for valid map composite entries, got diagnostics: {:?}",
        r.diagnostics
    );
}
