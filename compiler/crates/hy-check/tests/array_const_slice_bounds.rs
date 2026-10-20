use hy_check::check_file;
use hy_diag::Diagnostic;
use hy_syntax::parse_file;

fn check(src: &str) -> Vec<Diagnostic> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast).diagnostics
}

#[test]
fn test_array_slice_low_bound_past_len_rejected() {
    let src = r#"
package main

fn main() {
    var a [3]int
    _ = a[5:]
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("invalid slice index `5` (out of bounds for a 3-element array)"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_array_slice_high_bound_past_len_rejected() {
    let src = r#"
package main

fn main() {
    var a [3]int
    _ = a[:10]
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("invalid slice index `10` (out of bounds for a 3-element array)"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_array_slice_both_bounds_rejected() {
    let src = r#"
package main

fn main() {
    var a [3]int
    _ = a[2:4]
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("invalid slice index `4` (out of bounds for a 3-element array)"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_array_literal_slice_bound_past_len_rejected() {
    let src = r#"
package main

fn main() {
    b := [3]int{1, 2, 3}
    _ = b[4:]
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("invalid slice index `4` (out of bounds for a 3-element array)"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_inferred_array_literal_slice_bound_past_len_rejected() {
    let src = r#"
package main

fn main() {
    c := [...]int{1, 2, 3}
    _ = c[:5]
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("invalid slice index `5` (out of bounds for a 3-element array)"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_array_pointer_slice_bound_past_len_rejected() {
    let src = r#"
package main

fn main() {
    var a [3]int
    p := &a
    _ = p[5:]
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("invalid slice index `5` (out of bounds for a 3-element array)"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_typed_array_pointer_slice_bound_past_len_rejected() {
    let src = r#"
package main

fn main() {
    var p *[3]int
    _ = p[:4]
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E1002");
    assert!(
        diags[0]
            .message
            .contains("invalid slice index `4` (out of bounds for a 3-element array)"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_array_slice_exact_len_bounds_accepted() {
    let src = r#"
package main

fn main() {
    var a [3]int
    _ = a[:3]
    _ = a[3:]
    _ = a[0:3]
    _ = a[1:2]
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}

#[test]
fn test_slice_type_unbounded_accepted() {
    let src = r#"
package main

fn main() {
    s := []int{1, 2, 3}
    _ = s[:10]
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}
