//! array_literal_local_length_3599.rs — Regression test suite for #3599.
//!
//! #2771 rejects arrays of different length or element type, but previously
//! only when the local was declared with an explicit type. Locals initialised from
//! an array literal (`a := [3]int{..}` or `a := [...]int{..}`) recorded no type spelling,
//! so comparing or assigning them against an array of another length or element type
//! passed `hyb check` and failed `go build` downstream (Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), `hy-check` must reject these with `HYB-E1002`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e1002(body: &str) {
    let src = format!("package main\nimport \"fmt\"\nfn main() {{\n    {}\n}}\n", body);
    let r = check(&src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{}\nGot: {:?}",
        body,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected verdict Reject for:\n{}", body);
}

fn accepts_cleanly(body: &str) {
    let src = format!("package main\nimport \"fmt\"\nfn main() {{\n    {}\n}}\n", body);
    let r = check(&src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected 0 errors for:\n{}\nGot: {:?}",
        body,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected verdict Accept for:\n{}", body);
}

#[test]
fn test_issue_3599_all_table_rows_reject_with_e1002() {
    // Row 1: a := [3]int{1,2,3}; var b [4]int; a == b
    rejects_with_e1002("a := [3]int{1, 2, 3}\nvar b [4]int\nfmt.println(a == b)");

    // Row 2: var a [3]int; b := [4]int{1,2,3,4}; a == b
    rejects_with_e1002("var a [3]int\nb := [4]int{1, 2, 3, 4}\nfmt.println(a == b)");

    // Row 3: a := [3]int{..}; b := [4]int{..}; a != b
    rejects_with_e1002("a := [3]int{1, 2, 3}\nb := [4]int{1, 2, 3, 4}\nfmt.println(a != b)");

    // Row 4: a := [3]int{..}; var b [4]int; b = a
    rejects_with_e1002("a := [3]int{1, 2, 3}\nvar b [4]int\nb = a");

    // Row 5: a := [3]int{..}; b := [4]int{..}; b = a
    rejects_with_e1002("a := [3]int{1, 2, 3}\nb := [4]int{1, 2, 3, 4}\nb = a");

    // Row 6: a := [...]int{1,2,3}; b := [4]int{..}; a == b
    rejects_with_e1002("a := [...]int{1, 2, 3}\nb := [4]int{1, 2, 3, 4}\nfmt.println(a == b)");

    // Row 7: a := [3]int{..}; b := [3]int64{..}; a == b
    rejects_with_e1002("a := [3]int{1, 2, 3}\nb := [3]int64{1, 2, 3}\nfmt.println(a == b)");
}

#[test]
fn test_constant_array_length_and_literal_comparisons_3599() {
    // Constant array length in := binding: const N = 3; a := [N]int{1, 2, 3}; var b [4]int; a == b
    let const_src = r#"
package main
import "fmt"
const n = 3
fn main() {
    a := [n]int{1, 2, 3}
    var b [4]int
    fmt.println(a == b)
}
"#;
    let r = check(const_src);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for constant array length mismatch"
    );

    // Direct comparison between local and [...]int literal of mismatched length
    rejects_with_e1002("a := [4]int{1, 2, 3, 4}\nfmt.println(a == [...]int{1, 2, 3})");

    // Direct comparison between two array literals of mismatched length
    rejects_with_e1002("fmt.println([3]int{1, 2, 3} == [4]int{1, 2, 3, 4})");
    rejects_with_e1002("fmt.println([... ]int{1, 2, 3} == [4]int{1, 2, 3, 4})");

    // Direct assignment of mismatched literal to typed variable
    rejects_with_e1002("var b [4]int\nb = [3]int{1, 2, 3}");
}

#[test]
fn test_matching_arrays_stay_accepted_3599() {
    // Controls: identical length and element type must continue to accept cleanly
    accepts_cleanly("a := [3]int{1, 2, 3}\nvar b [3]int\nfmt.println(a == b)");
    accepts_cleanly("a := [3]int{1, 2, 3}\nb := [3]int{4, 5, 6}\nfmt.println(a == b)");
    accepts_cleanly("a := [...]int{1, 2, 3}\nb := [3]int{1, 2, 3}\nfmt.println(a == b)");
    accepts_cleanly("a := [3]int{1, 2, 3}\nvar b [3]int\nb = a\nfmt.println(b)");
    accepts_cleanly("a := [3]int{1, 2, 3}\nb := [3]int{0, 0, 0}\nb = a\nfmt.println(b)");
}
