use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    res.diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

fn check_verdict(src: &str) -> Verdict {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast).verdict
}

#[test]
fn option_scalar_width_mismatch_in_assignment_rejected() {
    let src = r#"
fn main() {
    var o1 option[int32]
    var o2 option[int64]
    o1 = o2
    _ = o1
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning option[int64] to option[int32], got: {:?}",
        codes
    );
}

#[test]
fn option_some_scalar_width_mismatch_in_assignment_rejected() {
    let src = r#"
fn main() {
    var op option[int32]
    var n int64 = 1
    op = some(n)
    _ = op
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning some(int64) to option[int32], got: {:?}",
        codes
    );
}

#[test]
fn option_some_scalar_width_mismatch_in_var_decl_rejected() {
    let src = r#"
fn main() {
    var n int64 = 1
    var op option[int32] = some(n)
    _ = op
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for initializing option[int32] with some(int64), got: {:?}",
        codes
    );
}

#[test]
fn option_scalar_width_mismatch_in_equality_rejected() {
    let src = r#"
fn main() {
    var o1 option[int32]
    var o2 option[int64]
    _ = o1 == o2
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for comparing option[int32] == option[int64], got: {:?}",
        codes
    );
}

#[test]
fn option_scalar_width_mismatch_in_return_rejected() {
    let src = r#"
fn ret() option[int32] {
    var n int64 = 1
    return some(n)
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for returning some(int64) from option[int32] fn, got: {:?}",
        codes
    );
}

#[test]
fn option_scalar_width_mismatch_in_fn_arg_rejected() {
    let src = r#"
fn f(op option[int32]) {
    _ = op
}

fn main() {
    var n int64 = 1
    f(some(n))
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for passing some(int64) to option[int32] parameter, got: {:?}",
        codes
    );
}

#[test]
fn option_array_length_mismatch_in_assignment_rejected() {
    let src = r#"
fn main() {
    var o1 option[[3]int]
    var a [4]int
    o1 = some(a)
    _ = o1
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning some([4]int) to option[[3]int], got: {:?}",
        codes
    );
}

#[test]
fn option_array_length_mismatch_in_equality_rejected() {
    let src = r#"
fn main() {
    var o1 option[[3]int]
    var o2 option[[4]int]
    _ = o1 == o2
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for comparing option[[3]int] == option[[4]int], got: {:?}",
        codes
    );
}

#[test]
fn option_float_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var o1 option[float32]
    var o2 option[float64]
    o1 = o2
    _ = o1
}
"#;
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning option[float64] to option[float32], got: {:?}",
        codes
    );
}

#[test]
fn option_matching_types_accepted() {
    let src = r#"
fn take_opt(op option[int32]) {
    _ = op
}

fn ret_opt() option[int32] {
    var n int32 = 1
    return some(n)
}

fn main() {
    var o1 option[int32]
    var o2 option[int32]
    o1 = o2
    _ = o1 == o2

    var n int32 = 5
    var o3 option[int32] = some(n)
    o1 = some(n)
    take_opt(some(n))
    _ = o3
    _ = ret_opt()
}
"#;
    assert_eq!(check_verdict(src), Verdict::Accept);
    assert!(error_codes(src).is_empty());
}
