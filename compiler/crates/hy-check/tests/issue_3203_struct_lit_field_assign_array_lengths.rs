//! Regression tests for issue #3203:
//! Struct composite literals, field assignments, and array/slice index assignments
//! must enforce fixed-size array lengths under HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn issue_3203_rejects_struct_and_assign_mismatches() {
    let cases = [
        // 1. Struct literal with sized array variable length mismatch ([3]int into [2]int)
        (
            "struct config {\n    arr [2]int\n}\nfn main() {\n    var a [3]int\n    _ = config{ arr: a }\n}\n",
            "struct literal with mismatched array variable",
        ),
        // 2. Struct literal with sized array literal length mismatch ([3]int into [2]int)
        (
            "struct config {\n    arr [2]int\n}\nfn main() {\n    _ = config{ arr: [3]int{1, 2, 3} }\n}\n",
            "struct literal with mismatched array literal",
        ),
        // 3. Struct field assignment with sized array variable length mismatch
        (
            "struct config {\n    arr [2]int\n}\nfn main() {\n    var c config\n    var a [3]int\n    c.arr = a\n}\n",
            "struct field assignment with mismatched array variable",
        ),
        // 4. Struct field assignment with sized array literal length mismatch
        (
            "struct config {\n    arr [2]int\n}\nfn main() {\n    var c config\n    c.arr = [3]int{1, 2, 3}\n}\n",
            "struct field assignment with mismatched array literal",
        ),
        // 5. Nested struct field assignment with sized array length mismatch
        (
            "struct inner {\n    arr [2]int\n}\nstruct outer {\n    in inner\n}\nfn main() {\n    var o outer\n    var a [3]int\n    o.in.arr = a\n}\n",
            "nested struct field assignment with mismatched array variable",
        ),
        // 6. Array index assignment with sized array variable length mismatch
        (
            "fn main() {\n    var grid [2][2]int\n    var row [3]int\n    grid[0] = row\n}\n",
            "array index assignment with mismatched array variable",
        ),
        // 7. Array index assignment with sized array literal length mismatch
        (
            "fn main() {\n    var grid [2][2]int\n    grid[0] = [3]int{1, 2, 3}\n}\n",
            "array index assignment with mismatched array literal",
        ),
        // 8. Slice index assignment with sized array length mismatch
        (
            "fn main() {\n    var slices [][2]int\n    slices[0] = [3]int{1, 2, 3}\n}\n",
            "slice index assignment with mismatched array literal",
        ),
    ];

    for (src, desc) in cases {
        let parsed = parse_file("test.hy", src).expect("parse must succeed");
        let result = check_file(&parsed);
        assert_eq!(
            result.verdict,
            Verdict::Reject,
            "Expected Reject for {desc}, but got Accept.\nSource:\n{src}"
        );
        let codes = error_codes(&result);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "Expected HYB-E1002 error for {desc}, got: {codes:?}\nSource:\n{src}"
        );
    }
}

#[test]
fn issue_3203_accepts_valid_struct_and_assign_cases() {
    let cases = [
        // 1. Struct literal with matching array variable
        (
            "struct config {\n    arr [2]int\n}\nfn main() {\n    var a [2]int\n    _ = config{ arr: a }\n}\n",
            "struct literal with matching array variable",
        ),
        // 2. Struct literal with matching array literal
        (
            "struct config {\n    arr [2]int\n}\nfn main() {\n    _ = config{ arr: [2]int{1, 2} }\n}\n",
            "struct literal with matching array literal",
        ),
        // 3. Struct field assignment with matching array variable
        (
            "struct config {\n    arr [2]int\n}\nfn main() {\n    var c config\n    var a [2]int\n    c.arr = a\n}\n",
            "struct field assignment with matching array variable",
        ),
        // 4. Array index assignment with matching array variable
        (
            "fn main() {\n    var grid [2][2]int\n    var row [2]int\n    grid[0] = row\n}\n",
            "array index assignment with matching array variable",
        ),
        // 5. Slice index assignment with matching array literal
        (
            "fn main() {\n    var slices [][2]int\n    slices[0] = [2]int{1, 2}\n}\n",
            "slice index assignment with matching array literal",
        ),
    ];

    for (src, desc) in cases {
        let parsed = parse_file("test.hy", src).expect("parse must succeed");
        let result = check_file(&parsed);
        let codes = error_codes(&result);
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "Expected Accept for {desc}, but got Reject with errors {codes:?}.\nSource:\n{src}"
        );
    }
}
