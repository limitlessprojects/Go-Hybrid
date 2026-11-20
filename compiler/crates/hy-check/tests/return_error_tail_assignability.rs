use hy_check::check_file;
use hy_diag::Diagnostic;
use hy_syntax::parse_file;

fn check(src: &str) -> Vec<Diagnostic> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast).diagnostics
}

#[test]
fn test_multi_value_call_with_mismatched_error_tail_rejected() {
    let src = r#"
fn pair() (int, int) {
    return 1, 2
}

fn worker() (int, error) {
    return pair()
}

fn main() {
    v, err := worker()
    _ = v
    _ = err
}
"#;
    let diags = check(src);
    assert_eq!(
        diags.len(),
        1,
        "expected exactly 1 diagnostic, got {:?}",
        diags
    );
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("cannot return a value of type `int` from a function declaring `error`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_direct_return_int_as_error_rejected() {
    let src = r#"
fn bad() error {
    return 42
}

fn main() {
    _ = bad()
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("cannot return a value of type `int` from a function declaring `error`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_direct_return_string_as_error_rejected() {
    let src = r#"
fn bad() error {
    return "some error"
}

fn main() {
    _ = bad()
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("cannot return a value of type `string` from a function declaring `error`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_direct_return_pair_mismatched_error_slot_rejected() {
    let src = r#"
fn bad_pair() (int, int) {
    return 1, 2
}

fn worker() (int, error) {
    return 1, 2
}

fn main() {
    a, b := worker()
    _ = a
    _ = b
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("cannot return a value of type `int` from a function declaring `error`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_multi_value_call_with_matching_error_tail_accepted() {
    let src = r#"
fn pair() (int, error) {
    return 1, nil
}

fn worker() (int, error) {
    return pair()
}

fn main() {
    v, err := worker()
    _ = v
    _ = err
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}

#[test]
fn test_single_value_call_into_error_tail_accepted() {
    let src = r#"
fn single() int {
    return 42
}

fn worker() (int, error) {
    return single()
}

fn main() {
    v, err := worker()
    _ = v
    _ = err
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}

#[test]
fn test_multi_value_call_into_longer_error_tail_signature_rejected() {
    let src = r#"
fn pair() (int, string) {
    return 1, "ok"
}

fn worker() (int, string, error) {
    return pair()
}

fn main() {
    a, b, err := worker()
    _ = a
    _ = b
    _ = err
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("multi-value call `pair` (2 results) in an error-tail function expecting 3 results"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_return_nil_in_error_slot_accepted() {
    let src = r#"
fn ok() (int, error) {
    return 1, nil
}

fn main() {
    v, err := ok()
    _ = v
    _ = err
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}
