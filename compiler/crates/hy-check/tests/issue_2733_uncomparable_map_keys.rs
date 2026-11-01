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
fn reject_slice_key_in_var_decl() {
    let src = "fn main() {\n    var m map[[]int]string\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn reject_map_key_in_var_decl() {
    let src = "fn main() {\n    var m map[map[string]int]bool\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn reject_fn_key_in_var_decl() {
    let src = "fn main() {\n    var m map[fn()]int\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn reject_slice_key_in_type_alias() {
    let src = "type m_alias = map[[]int]string\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn reject_slice_key_in_struct_field() {
    let src = "struct holder {\n    m map[[]string]int\n}\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn reject_slice_key_in_fn_param() {
    let src = "fn take(m map[[]int]string) {}\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn reject_slice_key_in_map_literal() {
    let src = "fn main() {\n    m := map[[]int]string{}\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn reject_nested_slice_in_array_key() {
    let src = "fn main() {\n    var m map[[2][]int]string\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn accept_valid_comparable_map_keys() {
    let src = r#"
struct point {
    x int
    y int
}

fn main() {
    var m1 map[int]string
    var m2 map[string]bool
    var m3 map[*int]int
    var m4 map[point]string
    var m5 map[[4]int]int
    _ = m1
    _ = m2
    _ = m3
    _ = m4
    _ = m5
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
