//! Regression tests for #2542:
//! verification of ordered types (HYB-E2005) and type matching (HYB-E1002) for `min` and `max`,
//! while properly permitting untyped integer constants with float arguments.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn min_max_unordered_operands_rejected_with_e2005() {
    let cases = [
        // 1. bool operand in min
        r#"
fn main() {
    _ = min(true, false)
}
"#,
        // 2. slice operand in max
        r#"
fn main() {
    s1 := []int{1}
    s2 := []int{2}
    _ = max(s1, s2)
}
"#,
        // 3. map operand in min
        r#"
fn main() {
    m1 := map[string]int{}
    m2 := map[string]int{}
    _ = min(m1, m2)
}
"#,
        // 4. chan operand in max
        r#"
fn main() {
    c1 := make(chan int, 1)
    c2 := make(chan int, 1)
    _ = max(c1, c2)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "unordered case #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E2005"),
            "unordered case #{idx} should emit HYB-E2005: {diags:?}\n{src}"
        );
    }
}

#[test]
fn min_max_mismatched_operand_types_rejected_with_e1002() {
    let cases = [
        // 1. string and int in min
        r#"
fn main() {
    _ = min("hello", 42)
}
"#,
        // 2. float and string in max
        r#"
fn main() {
    _ = max(3.14, "world")
}
"#,
        // 3. float and non-constant int variable in min
        r#"
fn main() {
    var i int = 1
    _ = min(1.5, i)
}
"#,
        // 4. non-constant int variable and float in max
        r#"
fn main() {
    var i int = 1
    _ = max(i, 1.5)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "mismatched case #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "mismatched case #{idx} should emit HYB-E1002: {diags:?}\n{src}"
        );
    }
}

#[test]
fn min_max_valid_ordered_operands_accepted() {
    let cases = [
        // 1. integers in min
        r#"
fn main() {
    _ = min(1, 2, 3)
}
"#,
        // 2. strings in max
        r#"
fn main() {
    _ = max("apple", "banana")
}
"#,
        // 3. floats in min
        r#"
fn main() {
    _ = min(1.5, 2.5)
}
"#,
        // 4. float and untyped int literal in min
        r#"
fn main() {
    _ = min(1.5, 2)
}
"#,
        // 5. untyped int literal and float in max
        r#"
fn main() {
    _ = max(2, 1.5)
}
"#,
        // 6. untyped int const and float in min
        r#"
const k = 10
fn main() {
    _ = min(1.5, k)
}
"#,
        // 7. identical concrete typed scalars
        r#"
fn main() {
    var a int32 = 1
    var b int32 = 2
    _ = min(a, b)
}
"#,
        // 8. concrete typed scalar with untyped constant
        r#"
fn main() {
    var a int32 = 1
    _ = min(a, 2)
}
"#,
        // 9. strings result used as string
        r#"
fn main() {
    s := min("a", "b")
    _ = s + "c"
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "compatible case #{idx} should accept, got Reject with diags: {diags:?}\n{src}"
        );
    }
}

#[test]
fn min_max_mismatched_scalar_types_rejected_with_e1002() {
    let cases = [
        // 1. int32 and int64 in min
        r#"
fn main() {
    var a int32 = 1
    var b int64 = 2
    _ = min(a, b)
}
"#,
        // 2. float32 and float64 in max
        r#"
fn main() {
    var a float32 = 1.0
    var b float64 = 2.0
    _ = max(a, b)
}
"#,
        // 3. uint and int in min
        r#"
fn main() {
    var a uint = 1
    var b int = 2
    _ = min(a, b)
}
"#,
        // 4. constant prefix with mismatched scalars
        r#"
fn main() {
    var a int32 = 1
    var b int64 = 2
    _ = min(0, a, b)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "mismatched scalar case #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "mismatched scalar case #{idx} should emit HYB-E1002: {diags:?}\n{src}"
        );
    }
}

#[test]
fn min_max_result_kind_and_scalar_propagated() {
    let cases = [
        // 1. min(int, int) assigned to string variable
        r#"
fn main() {
    var s string = min(1, 2)
    _ = s
}
"#,
        // 2. min(int, int) combined with string binary op
        r#"
fn main() {
    _ = min(1, 2) + "hello"
}
"#,
        // 3. min(int32, int32) assigned to int64 variable
        r#"
fn main() {
    var a int32 = 1
    var b int32 = 2
    m := min(a, b)
    var c int64 = m
    _ = c
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "result kind propagation case #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "result kind propagation case #{idx} should emit HYB-E1002: {diags:?}\n{src}"
        );
    }
}
