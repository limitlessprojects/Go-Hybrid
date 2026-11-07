//! N1 must apply to map keys, including when the value is also invalid.
use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("test.hy", src).expect("test snippet must parse"))
}

fn nil_errors(src: &str) -> Vec<hy_diag::Diagnostic> {
    let result = check(src);
    assert_eq!(
        result.verdict,
        Verdict::Reject,
        "accepted invalid nil key: {src}"
    );
    result
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E4001")
        .collect()
}

#[test]
fn rejects_nil_keys_for_hybrid_declared_types() {
    for ty in [
        "int",
        "string",
        "bool",
        "[2]int",
        "*int",
        "key_alias",
        "key_struct",
    ] {
        let src = format!("type key_alias = int\nstruct key_struct {{ n int }}\nfn main() {{\n m := map[{ty}]int{{nil: 1}}\n fmt.println(m)\n}}\n");
        assert_eq!(nil_errors(&src).len(), 1, "wrong nil diagnostics for {ty}");
    }
}

#[test]
fn rejects_parenthesized_nil_keys() {
    let src = "fn main() {\n m := map[int]int{((nil)): 1}\n fmt.println(m)\n}\n";
    assert_eq!(nil_errors(src).len(), 1);
}

#[test]
fn diagnoses_key_and_value_independently() {
    let src = "fn main() {\n m := map[int]int{nil: nil}\n fmt.println(m)\n}\n";
    assert_eq!(nil_errors(src).len(), 2);
}

#[test]
fn rejects_nested_nil_key_even_if_outer_value_is_nil() {
    let src = "fn main() {\n m := map[[1]map[int]int]int{[1]map[int]int{map[int]int{nil: 1}}: nil}\n fmt.println(m)\n}\n";
    assert_eq!(nil_errors(src).len(), 2);
}

#[test]
fn rejects_nil_keys_in_returns_arguments_and_managed_blocks() {
    for src in [
        "fn bad() map[int]int {\n return map[int]int{nil: 1}\n}\n",
        "fn take(m map[int]int) {}\nfn main() {\n take(map[int]int{nil: 1})\n}\n",
        "fn main() {\n managed {\n m := map[int]int{nil: 1}\n fmt.println(m)\n }\n}\n",
    ] {
        assert!(!nil_errors(src).is_empty());
    }
}

#[test]
fn preserves_valid_keys_and_existing_nil_value_rules() {
    let valid = "fn main() {\n a := map[int]int{0: 1}\n b := map[string]int{\"\": 2}\n c := map[bool]int{false: 3}\n fmt.println(a, b, c)\n}\n";
    let result = check(valid);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
    for literal in [
        "map[int]int{1: nil}",
        "[]int{nil}",
        "[1]string{nil}",
        "map[int][]int{1: nil}",
    ] {
        let src = format!("fn main() {{\n m := {literal}\n fmt.println(m)\n}}\n");
        assert_eq!(nil_errors(&src).len(), 1);
    }
}

#[test]
fn preserves_error_and_foreign_nil_key_homes() {
    for ty in ["error", "error_alias", "io.reader"] {
        let src = format!("type error_alias = error\nfn main() {{\n m := map[{ty}]int{{nil: 1}}\n fmt.println(m)\n}}\n");
        let result = check(&src);
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "{ty}: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn shadowing_error_does_not_grant_nil_to_a_hybrid_struct() {
    let src = "struct error { n int }\nfn main() {\n m := map[error]int{nil: 1}\n fmt.println(m)\n}\n";
    assert_eq!(nil_errors(src).len(), 1);
}
