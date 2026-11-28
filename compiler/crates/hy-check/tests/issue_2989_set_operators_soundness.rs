//! issue_2989_set_operators_soundness.rs — regression tests for Issue #2989:
//! verifies that arithmetic/bitwise/relational operators, unary operators (!, -),
//! compound assignments, and builtins (append, copy, min, max, cap, close, clear)
//! on set[T] are rejected with HYB-E1002 / HYB-E2005 (fixes #2989, Row 13 BUILDABLE).

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

fn assert_reject_code(src: &str, expected_code: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for snippet:\n{src}\ngot Accept with diagnostics: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).contains(&expected_code.to_string()),
        "expected error code {expected_code} for snippet:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn binary_arithmetic_operators_on_set_rejected_with_e1002() {
    let ops = ["+", "-", "*", "/", "%"];
    for op in ops {
        let src = format!(
            "fn main() {{\n    var s set[int]\n    _ = s {} 1\n}}\n",
            op
        );
        assert_reject_code(&src, "HYB-E1002");

        let src_rev = format!(
            "fn main() {{\n    var s set[int]\n    _ = 1 {} s\n}}\n",
            op
        );
        assert_reject_code(&src_rev, "HYB-E1002");
    }
}

#[test]
fn binary_bitwise_and_relational_operators_on_set_rejected_with_e1002() {
    let ops = ["&", "|", "^", "<<", ">>", "<", "<=", ">", ">="];
    for op in ops {
        let src = format!(
            "fn main() {{\n    var s set[int]\n    _ = s {} 1\n}}\n",
            op
        );
        assert_reject_code(&src, "HYB-E1002");
    }

    let bool_src = "fn main() {\n    var s set[int]\n    _ = s && true\n}\n";
    assert_reject_code(bool_src, "HYB-E1002");

    let bool_src2 = "fn main() {\n    var s set[int]\n    _ = false || s\n}\n";
    assert_reject_code(bool_src2, "HYB-E1002");
}

#[test]
fn unary_operators_on_set_rejected_with_e1002() {
    let not_src = "fn main() {\n    var s set[int]\n    _ = !s\n}\n";
    assert_reject_code(not_src, "HYB-E1002");

    let neg_src = "fn main() {\n    var s set[int]\n    _ = -s\n}\n";
    assert_reject_code(neg_src, "HYB-E1002");
}

#[test]
fn compound_assignment_and_inc_dec_on_set_rejected_with_e1002() {
    let inc_src = "fn main() {\n    var s set[int]\n    s++\n}\n";
    assert_reject_code(inc_src, "HYB-E1002");

    let dec_src = "fn main() {\n    var s set[int]\n    s--\n}\n";
    assert_reject_code(dec_src, "HYB-E1002");

    let add_assign_src = "fn main() {\n    var s set[int]\n    s += 1\n}\n";
    assert_reject_code(add_assign_src, "HYB-E1002");

    let sub_assign_src = "fn main() {\n    var s set[int]\n    s -= 1\n}\n";
    assert_reject_code(sub_assign_src, "HYB-E1002");
}

#[test]
fn builtins_on_set_rejected_with_e2005() {
    let append_src = "fn main() {\n    var s set[int]\n    _ = append(s, 1)\n}\n";
    assert_reject_code(append_src, "HYB-E2005");

    let copy_src1 = "fn main() {\n    var s set[int]\n    var sl []int\n    _ = copy(sl, s)\n}\n";
    assert_reject_code(copy_src1, "HYB-E2005");

    let copy_src2 = "fn main() {\n    var s set[int]\n    var sl []int\n    _ = copy(s, sl)\n}\n";
    assert_reject_code(copy_src2, "HYB-E2005");

    let min_src = "fn main() {\n    var s1 set[int]\n    var s2 set[int]\n    _ = min(s1, s2)\n}\n";
    assert_reject_code(min_src, "HYB-E2005");

    let max_src = "fn main() {\n    var s1 set[int]\n    var s2 set[int]\n    _ = max(s1, s2)\n}\n";
    assert_reject_code(max_src, "HYB-E2005");

    let cap_src = "fn main() {\n    var s set[int]\n    _ = cap(s)\n}\n";
    assert_reject_code(cap_src, "HYB-E2005");

    let close_src = "fn main() {\n    var s set[int]\n    close(s)\n}\n";
    assert_reject_code(close_src, "HYB-E2005");

    let clear_src = "fn main() {\n    var s set[int]\n    clear(s)\n}\n";
    assert_reject_code(clear_src, "HYB-E2005");
}

#[test]
fn test_issue_2989_reproduction_snippet() {
    let src = r#"
fn main() {
    var s1 set[int]
    var s2 set[int]
    var sl []int

    _ = s1 + 1
    _ = -s1
    _ = !s1
    _ = s1 < 1
    _ = append(s1, 1)
    _ = copy(sl, s1)
    _ = min(s1, s2)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert_eq!(codes.len(), 7, "expected 7 errors, got: {:?}", r.diagnostics);
}

#[test]
fn positive_controls_valid_set_usage_accepted() {
    let src = "\
fn main() {
    var s set[int]
    s.insert(1)
    _ = s.contains(1)
    s.remove(1)
    delete(s, 1)
    _ = len(s)
    for v in s {
        _ = v
    }
    p := &s
    _ = p
    var s2 set[int]
    s2 = s
    _ = s2
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for valid set operations, got: {:?}",
        r.diagnostics
    );
}


