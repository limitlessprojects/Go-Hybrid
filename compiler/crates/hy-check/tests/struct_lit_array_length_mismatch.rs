//! struct_lit_array_length_mismatch.rs — issue #3203.
//!
//! Struct composite literals (`t{ field: value }`) and assignments (`target = value`,
//! including field assignment `obj.field = value` and index assignment `grid[i] = value`)
//! must verify fixed-size array lengths between the declared field/target type and the
//! value. Sized arrays with different lengths must be rejected with HYB-E1002.

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
fn different_array_lengths_in_struct_literal_and_assign_reject() {
    for (name, src) in [
        (
            "struct literal with mismatched array variable",
            "struct box_t {\n    arr [3]int\n}\nfn main() {\n    var a [4]int\n    b := box_t{arr: a}\n    _ = b\n}\n",
        ),
        (
            "struct literal with mismatched array literal",
            "struct box_t {\n    arr [3]int\n}\nfn main() {\n    b := box_t{arr: [4]int{1, 2, 3, 4}}\n    _ = b\n}\n",
        ),
        (
            "struct field assignment with mismatched array variable",
            "struct box_t {\n    arr [3]int\n}\nfn main() {\n    var b box_t\n    var a [4]int\n    b.arr = a\n}\n",
        ),
        (
            "struct field assignment with mismatched array literal",
            "struct box_t {\n    arr [3]int\n}\nfn main() {\n    var b box_t\n    b.arr = [4]int{1, 2, 3, 4}\n}\n",
        ),
        (
            "index assignment with mismatched array variable",
            "fn main() {\n    var grid [2][2]int\n    var row [3]int\n    grid[0] = row\n}\n",
        ),
        (
            "generic struct literal with mismatched array variable",
            "struct wrapper[t] {\n    val t\n}\nfn main() {\n    var a [4]int\n    w := wrapper[[3]int]{val: a}\n    _ = w\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn matching_array_lengths_in_struct_literal_and_assign_accept() {
    for (name, src) in [
        (
            "struct literal with matching array variable",
            "struct box_t {\n    arr [3]int\n}\nfn main() {\n    var a [3]int\n    b := box_t{arr: a}\n    _ = b\n}\n",
        ),
        (
            "struct literal with matching array literal",
            "struct box_t {\n    arr [3]int\n}\nfn main() {\n    b := box_t{arr: [3]int{1, 2, 3}}\n    _ = b\n}\n",
        ),
        (
            "struct field assignment with matching array variable",
            "struct box_t {\n    arr [3]int\n}\nfn main() {\n    var b box_t\n    var a [3]int\n    b.arr = a\n}\n",
        ),
        (
            "index assignment with matching array variable",
            "fn main() {\n    var grid [2][2]int\n    var row [2]int\n    grid[0] = row\n}\n",
        ),
        (
            "generic struct literal with matching array variable",
            "struct wrapper[t] {\n    val t\n}\nfn main() {\n    var a [3]int\n    w := wrapper[[3]int]{val: a}\n    _ = w\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
