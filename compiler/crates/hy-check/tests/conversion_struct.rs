//! conversion_struct.rs — regression tests for #1879: distinct named structs
//! are nominal types and cannot be converted with `B(a)`.

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

const PRELUDE: &str = "struct a {\n    x int\n}\n\nstruct b {\n    x int\n}\n\n";

#[test]
fn conversion_between_distinct_structs_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    v := a{{x: 1}}\n    w := b(v)\n    _ = w\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `b(a_value)`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn conversion_to_same_struct_type_accepts() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    v := a{{x: 1}}\n    w := a(v)\n    _ = w\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "`a(a_value)` is the same type and must stay accepted, got {cs:?}"
    );
}

#[test]
fn scalar_to_struct_conversion_is_e1002() {
    let r = check(&format!("{PRELUDE}fn main() {{\n    w := a(1)\n    _ = w\n}}\n"));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `a(1)`, got {cs:?}"
    );
}
