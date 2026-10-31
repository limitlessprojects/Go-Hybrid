//! struct_field_value.rs — regression tests for #1853: every named struct
//! literal field value is a single-value slot.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const PRELUDE: &str = "struct pair {\n    a int\n    b int\n}\n\nfn two_ints() (int, int) {\n    return 1, 2\n}\n\nfn nothing() {\n}\n\n";

#[test]
fn multi_value_struct_field_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    p := pair{{a: two_ints(), b: 3}}\n    _ = p\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `pair{{a: two_ints()}}`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn void_struct_field_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    p := pair{{a: nothing(), b: 3}}\n    _ = p\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void struct field value, got {cs:?}"
    );
}

#[test]
fn ordinary_struct_literal_accepts() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    p := pair{{a: 1, b: 2}}\n    _ = p\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "an ordinary struct literal must stay accepted, got {cs:?}"
    );
}
