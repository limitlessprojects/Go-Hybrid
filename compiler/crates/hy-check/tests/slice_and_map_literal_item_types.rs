//! slice_and_map_literal_item_types.rs — tests for slice and map literal
//! item/key/value type checking (HYB-E1002 / BUILDABLE).
//!
//! Validates:
//! 1. Slice literal element type mismatches: `[]int{"hello"}`.
//! 2. Map literal key type mismatches: `map[string]int{123: 1}`.
//! 3. Map literal value type mismatches: `map[string]int{"foo": "bar"}`.
//! 4. Valid composite literals (matching types, untyped float constants, nested composites) stay cleanly accepted.

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

fn assert_ok(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean accept for {}, got errors: {:?}",
        what,
        error_codes(&r)
    );
}

#[test]
fn slice_literal_item_type_mismatch_rejected() {
    let src1 = "fn main() {\n    s := []int{\"hello\"}\n    fmt.println(\"{s}\")\n}\n";
    assert_rejects_e1002(&check(src1), "[]int{\"hello\"}");

    let src2 = "fn main() {\n    s := []string{123}\n    fmt.println(\"{s}\")\n}\n";
    assert_rejects_e1002(&check(src2), "[]string{123}");

    let src3 = "fn main() {\n    s := []bool{1}\n    fmt.println(\"{s}\")\n}\n";
    assert_rejects_e1002(&check(src3), "[]bool{1}");
}

#[test]
fn map_literal_key_type_mismatch_rejected() {
    let src1 = "fn main() {\n    m := map[string]int{123: 1}\n    fmt.println(\"{m}\")\n}\n";
    assert_rejects_e1002(&check(src1), "map[string]int{123: 1}");

    let src2 = "fn main() {\n    m := map[int]string{\"foo\": \"bar\"}\n    fmt.println(\"{m}\")\n}\n";
    assert_rejects_e1002(&check(src2), "map[int]string{\"foo\": \"bar\"}");
}

#[test]
fn map_literal_value_type_mismatch_rejected() {
    let src1 = "fn main() {\n    m := map[string]int{\"foo\": \"bar\"}\n    fmt.println(\"{m}\")\n}\n";
    assert_rejects_e1002(&check(src1), "map[string]int{\"foo\": \"bar\"}");

    let src2 = "fn main() {\n    m := map[string]bool{\"foo\": 123}\n    fmt.println(\"{m}\")\n}\n";
    assert_rejects_e1002(&check(src2), "map[string]bool{\"foo\": 123}");
}

#[test]
fn valid_slice_and_map_literals_stay_accepted() {
    let src = r#"
fn main() {
    s := []int{1, 2, 3}
    fmt.println("{s}")
    m := map[string]int{"foo": 1, "bar": 2}
    fmt.println("{m}")
    m2 := map[string]float64{"pi": 3.14, "int_const": 1}
    fmt.println("{m2}")
    nested := [][]int{{1, 2}, {3, 4}}
    fmt.println("{nested}")
    m_nested := map[string][]int{"k": {1, 2}}
    fmt.println("{m_nested}")
}
"#;
    assert_ok(src, "valid slice and map composite literals");
}

#[test]
fn type_elided_struct_literals_checked_for_unknown_and_duplicate_fields_issue_2098() {
    // 1. Unknown field in slice elided struct
    let src1 = r#"
struct item {
    count int
    label string
}
fn main() {
    s := []item{
        {unknown_field: 1},
    }
    _ = s
}
"#;
    assert_rejects_e1002(&check(src1), "unknown field in slice elided struct");

    // 2. Duplicate field in slice elided struct
    let src2 = r#"
struct item {
    count int
    label string
}
fn main() {
    s := []item{
        {count: 1, count: 2},
    }
    _ = s
}
"#;
    assert_rejects_e1002(&check(src2), "duplicate field in slice elided struct");

    // 3. Field type mismatch in slice elided struct
    let src3 = r#"
struct item {
    count int
    label string
}
fn main() {
    s := []item{
        {count: "wrong_type"},
    }
    _ = s
}
"#;
    assert_rejects_e1002(&check(src3), "field type mismatch in slice elided struct");

    // 4. Elided struct in scalar slice
    let src4 = r#"
fn main() {
    s := []int{{x: 1}}
    _ = s
}
"#;
    assert_rejects_e1002(&check(src4), "elided struct in scalar slice");

    // 5. Elided struct in scalar map value
    let src5 = r#"
fn main() {
    m := map[string]int{"k": {x: 1}}
    _ = m
}
"#;
    assert_rejects_e1002(&check(src5), "elided struct in scalar map value");

    // 6. Valid elided struct literals pass cleanly
    let src_ok = r#"
struct item {
    count int
    label string
}
fn main() {
    s := []item{
        {count: 1, label: "ok"},
    }
    _ = s
}
"#;
    assert_ok(src_ok, "valid elided struct literals in slice");
}
