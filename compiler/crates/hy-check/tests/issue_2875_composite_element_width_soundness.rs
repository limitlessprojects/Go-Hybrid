//! issue_2875_composite_element_width_soundness.rs — regression tests for #2875 (Row 13 BUILDABLE).
//!
//! Verification that composite types differing only in scalar element width
//! ([]i64 vs []int, [2]i64 vs [2]int, map[string]i64 vs map[string]int, *i64 vs *int)
//! and mixed-width min/max calls reject with HYB-E1002 across var declarations,
//! assignments, call arguments, and return statements.
//! Identical composite types and canonical aliases (i64 == int64, f32 == float32) remain accepted.

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
fn composite_element_width_mismatches_reject() {
    let cases = [
        // 1. Slice var initialization with mismatched element scalar ([]i64 -> []int)
        (
            "slice var init []i64 -> []int",
            r#"
fn main() {
    var a []i64
    var b []int = a
    _ = b
}
"#,
        ),
        // 2. Map var initialization with mismatched value scalar (map[string]i64 -> map[string]int)
        (
            "map var init map[string]i64 -> map[string]int",
            r#"
fn main() {
    var a map[string]i64
    var b map[string]int = a
    _ = b
}
"#,
        ),
        // 3. Pointer var initialization with mismatched base scalar (*i64 -> *int via address-of)
        (
            "pointer var init *i64 -> *int",
            r#"
fn main() {
    var v i64 = 1
    p := &v
    var q *int = p
    _ = q
}
"#,
        ),
        // 4. Sized array var initialization with mismatched element scalar ([2]i64 -> [2]int)
        (
            "array var init [2]i64 -> [2]int",
            r#"
fn main() {
    var a [2]i64
    var b [2]int = a
    _ = b
}
"#,
        ),
        // 5. Function call argument with mismatched slice element scalar
        (
            "call arg []i64 to param []int",
            r#"
fn take(xs []int) int {
    return len(xs)
}
fn main() {
    var a []i64
    _ = take(a)
}
"#,
        ),
        // 6. Return statement with mismatched slice element scalar
        (
            "return []i64 from fn declaring []int",
            r#"
fn mk() []int {
    var a []i64
    return a
}
fn main() {
    _ = mk()
}
"#,
        ),
        // 7. Plain assignment with mismatched slice element scalar
        (
            "assignment c = a ([]int = []i64)",
            r#"
fn main() {
    var a []i64
    var c []int
    c = a
    _ = c
}
"#,
        ),
        // 8. Plain assignment with mismatched map value scalar
        (
            "assignment c = a (map[string]int = map[string]i64)",
            r#"
fn main() {
    var a map[string]i64
    var c map[string]int
    c = a
    _ = c
}
"#,
        ),
        // 9. Plain assignment with mismatched pointer base scalar
        (
            "assignment q = p (*int = *i64)",
            r#"
fn main() {
    var v i64 = 1
    p := &v
    var q *int
    q = p
    _ = q
}
"#,
        ),
        // 10. Pointer equality comparison with mismatched base scalar
        (
            "pointer comparison *int64 == *int",
            r#"
fn main() {
    var p1 *int64
    var p2 *int
    _ = p1 == p2
}
"#,
        ),
        // 11. Sized array equality comparison with mismatched element scalar
        (
            "array comparison [2]int64 == [2]int",
            r#"
fn main() {
    var a1 [2]int64
    var a2 [2]int
    _ = a1 == a2
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "case should reject: {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case {name} should emit HYB-E1002: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn min_max_mixed_typed_scalars_reject() {
    let cases = [
        // 1. min with mixed typed int operands (i64 and int)
        (
            "min(i64, int)",
            r#"
fn main() {
    var a i64 = 1
    var b int = 2
    _ = min(a, b)
}
"#,
        ),
        // 2. max with mixed typed int operands (int and i64)
        (
            "max(int, i64)",
            r#"
fn main() {
    var a i64 = 1
    var b int = 2
    _ = max(b, a)
}
"#,
        ),
        // 3. max with mixed typed float operands (f32 and f64)
        (
            "max(f32, f64)",
            r#"
fn main() {
    var a f32 = 1.0
    var b f64 = 2.0
    _ = max(a, b)
}
"#,
        ),
        // 4. min with mixed typed float operands (f64 and f32)
        (
            "min(f64, f32)",
            r#"
fn main() {
    var a f32 = 1.0
    var b f64 = 2.0
    _ = min(b, a)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "case should reject: {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case {name} should emit HYB-E1002: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn identical_and_aliased_composite_types_accepted() {
    let cases = [
        // 1. Identical slice types
        (
            "identical slices []i64 -> []i64",
            r#"
fn main() {
    var a []i64
    var b []i64 = a
    _ = b
}
"#,
        ),
        // 2. Canonical slice alias ([]int64 and []i64)
        (
            "aliased slices []int64 -> []i64",
            r#"
fn main() {
    var a []int64
    var b []i64 = a
    _ = b
}
"#,
        ),
        // 3. Identical map types
        (
            "identical maps map[string]i64 -> map[string]i64",
            r#"
fn main() {
    var a map[string]i64
    var b map[string]i64 = a
    _ = b
}
"#,
        ),
        // 4. Aliased map types (map[string]int64 and map[string]i64)
        (
            "aliased maps map[string]int64 -> map[string]i64",
            r#"
fn main() {
    var a map[string]int64
    var b map[string]i64 = a
    _ = b
}
"#,
        ),
        // 5. Identical pointer types
        (
            "identical pointers *i64 -> *i64",
            r#"
fn main() {
    var v i64 = 1
    p := &v
    var q *i64 = p
    _ = q
}
"#,
        ),
        // 6. Aliased pointer types (*int64 and *i64)
        (
            "aliased pointers *int64 -> *i64",
            r#"
fn main() {
    var v int64 = 1
    p := &v
    var q *i64 = p
    _ = q
}
"#,
        ),
        // 7. Identical array types
        (
            "identical arrays [2]i64 -> [2]i64",
            r#"
fn main() {
    var a [2]i64
    var b [2]i64 = a
    _ = b
}
"#,
        ),
        // 8. Aliased array types ([2]int64 and [2]i64)
        (
            "aliased arrays [2]int64 -> [2]i64",
            r#"
fn main() {
    var a [2]int64
    var b [2]i64 = a
    _ = b
}
"#,
        ),
        // 9. min with identical typed operands
        (
            "min(i64, i64)",
            r#"
fn main() {
    var a i64 = 1
    var b i64 = 2
    _ = min(a, b)
}
"#,
        ),
        // 10. min with aliased typed operands (i64 and int64)
        (
            "min(i64, int64)",
            r#"
fn main() {
    var a i64 = 1
    var b int64 = 2
    _ = min(a, b)
}
"#,
        ),
        // 11. min with typed operand and untyped literal
        (
            "min(i64, untyped int literal)",
            r#"
fn main() {
    var a i64 = 1
    _ = min(a, 2)
}
"#,
        ),
        // 12. max with typed float and untyped float literal
        (
            "max(f64, untyped float literal)",
            r#"
fn main() {
    var a f64 = 1.0
    _ = max(a, 2.5)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "case should accept: {name}");
        assert!(
            error_codes(&r).is_empty(),
            "case {name} should have no errors: {:?}",
            r.diagnostics
        );
    }
}
