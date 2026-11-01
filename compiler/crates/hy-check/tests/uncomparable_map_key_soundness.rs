//! uncomparable_map_key_soundness.rs — regression tests for Issue #2733:
//! verification that uncomparable map key types (slices, maps, functions) in variable declarations,
//! type definitions, struct fields, function signatures, and composite literals are rejected with HYB-E1002
//! under Row 13 (BUILDABLE), adhering to Go spec §Map types.

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
fn uncomparable_slice_map_keys_rejected() {
    // 1. var m map[[]int]string
    let src1 = "fn main() {\n    var m map[[]int]string\n    _ = m\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. map[[]string]int{} composite literal
    let src2 = "fn main() {\n    _ = map[[]string]int{}\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. Array of slices as map key: [2][]int
    let src3 = "fn main() {\n    var m map[[2][]int]bool\n    _ = m\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn uncomparable_map_map_keys_rejected() {
    // 1. var m map[map[string]int]bool
    let src1 = "fn main() {\n    var m map[map[string]int]bool\n    _ = m\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. type alias with map key
    let src2 = "type bad_map = map[map[int]int]string\nfn main() {}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. map literal with map key
    let src3 = "fn main() {\n    _ = map[map[string]int]string{}\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn uncomparable_function_map_keys_rejected() {
    // 1. function parameter with fn() key
    let src1 = "fn process(m map[fn()]int) {}\nfn main() {}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. struct field with fn(int) int key
    let src2 = "struct container {\n    callbacks map[fn(int) int]string\n}\nfn main() {}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn comparable_map_keys_stay_accepted() {
    let src = r#"
struct valid_struct {
    m1 map[string]int
    m2 map[int]string
    m3 map[bool]float64
    m4 map[chan int]string
    m5 map[[3]int]string
}

fn handle_map(m map[string]int) map[int]string {
    return map[int]string{}
}

fn main() {
    var m map[string]int
    _ = m
    _ = map[string]int{"ok": 1}
    _ = map[int]bool{1: true, 2: false}
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
}
