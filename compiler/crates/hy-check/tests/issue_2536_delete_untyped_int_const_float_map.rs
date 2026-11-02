//! Tests for #2536:
//! `delete(m, k)` accepts valid untyped integer constants on float-keyed maps,
//! while continuing to reject typed integer variables/constants and mismatched key types with HYB-E2005.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn delete_key_untyped_int_constants_on_float_map_accepted() {
    let cases = [
        // 1. Untyped int literal on float map
        r#"
fn main() {
    var m map[float64]string
    delete(m, 1)
}
"#,
        // 2. Untyped int zero on float map
        r#"
fn main() {
    var m map[float64]int
    delete(m, 0)
}
"#,
        // 3. File-level named untyped int const on float map
        r#"
const k = 42
fn main() {
    var m map[float64]string
    delete(m, k)
}
"#,
        // 4. Function-local named untyped int const on float map
        r#"
fn main() {
    const local_k = 10
    var m map[float64]string
    delete(m, local_k)
}
"#,
        // 5. Binary arithmetic constant expression of untyped ints on float map
        r#"
fn main() {
    var m map[float64]string
    delete(m, 1 + 2)
}
"#,
        // 6. Unary negative constant expression of untyped int on float map
        r#"
fn main() {
    var m map[float64]string
    delete(m, -5)
}
"#,
        // 7. Float literal on float map
        r#"
fn main() {
    var m map[float64]string
    delete(m, 1.5)
}
"#,
        // 8. Float variable on float map
        r#"
fn main() {
    var m map[float64]string
    var f float64 = 2.5
    delete(m, f)
}
"#,
        // 9. Int variable on int map
        r#"
fn main() {
    var m map[int]string
    var i int = 1
    delete(m, i)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "compatible delete key case #{idx} should accept, got Reject with diags: {diags:?}\n{src}"
        );
    }
}

#[test]
fn delete_key_typed_int_and_conflicting_types_rejected_e2005() {
    let cases = [
        // 1. Non-constant int variable on float map
        r#"
fn main() {
    var m map[float64]string
    var i int = 1
    delete(m, i)
}
"#,
        // 2. File-level typed int const on float map
        r#"
const k int = 1
fn main() {
    var m map[float64]string
    delete(m, k)
}
"#,
        // 3. Function-local typed int const on float map
        r#"
fn main() {
    const local_k int = 1
    var m map[float64]string
    delete(m, local_k)
}
"#,
        // 4. Float literal on int map
        r#"
fn main() {
    var m map[int]string
    delete(m, 1.5)
}
"#,
        // 5. Float variable on int map
        r#"
fn main() {
    var m map[int]string
    var f float64 = 1.5
    delete(m, f)
}
"#,
        // 6. String literal on float map
        r#"
fn main() {
    var m map[float64]string
    delete(m, "bad")
}
"#,
        // 7. String variable on float map
        r#"
fn main() {
    var m map[float64]string
    var s string = "bad"
    delete(m, s)
}
"#,
        // 8. Bool literal on int map
        r#"
fn main() {
    var m map[int]string
    delete(m, true)
}
"#,
        // 9. String literal on int map
        r#"
fn main() {
    var m map[int]string
    delete(m, "1")
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "incompatible delete key case #{idx} should reject, got Accept:\n{src}"
        );
        let has_e2005 = diags.iter().any(|d| d.code == "HYB-E2005");
        assert!(
            has_e2005,
            "incompatible delete key case #{idx} should emit HYB-E2005, got: {diags:?}\n{src}"
        );
    }
}
