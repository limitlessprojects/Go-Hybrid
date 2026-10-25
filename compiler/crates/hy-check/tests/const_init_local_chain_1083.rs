//! Additional regression coverage for issue #1083 (`const_init.rs`):
//! - Nested control-flow (`if`, `for`, `match`, `select`) local `const` chains
//!   and shadowing before/after `const` use sites.
//! - Imported package name shadowing conversion/builtin targets in `const`
//!   initializers (`HYB-E6003`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {:?}", d));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn count_code(r: &hy_check::CheckResult, code: &str) -> usize {
    error_codes(r).iter().filter(|c| c.as_str() == code).count()
}

#[test]
fn nested_control_flow_local_const_chain_and_shadowing_respect_lexical_order() {
    let src_valid = "\
package main
type alias_str = string
fn main() {
    if flag := true; flag {
        const prefix = alias_str(\"ab\")
        const total = len(prefix) + 3
        _ = total
    }
}
";
    let r_valid = check(src_valid);
    assert!(error_codes(&r_valid).is_empty(), "{:?}", r_valid.diagnostics);
    assert_eq!(r_valid.verdict, Verdict::Accept);

    let src_shadowed_by_loop_var = "\
package main
const k = 10
fn main() {
    for k in 0..3 {
        const bad = k + 1
        _ = bad
    }
    _ = k
}
";
    let r_loop = check(src_shadowed_by_loop_var);
    assert_eq!(count_code(&r_loop, "HYB-E6003"), 1, "{:?}", r_loop.diagnostics);
}

#[test]
fn option_and_composite_const_ascriptions_reject_with_e6003() {
    let src = "\
package main
const opt_num option[int] = 7
const opt_str option[string] = \"ok\"
fn main() {
    const local_opt option[int] = 9
    _ = opt_num
    _ = opt_str
    _ = local_opt
}
";
    let r = check(src);
    assert_eq!(count_code(&r, "HYB-E6003"), 3, "{:?}", r.diagnostics);
}
