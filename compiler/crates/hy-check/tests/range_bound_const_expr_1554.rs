//! #1554 — `for i in lo..hi` only classified bare `const` literals as
//! non-integer bounds. A compound file-`const` initializer (`1.5 + 0.5`,
//! `!false`) carries no type ascription, so `shape_of_expr` saw `Unknown`
//! and `hyb check` accepted the range; the emitted Go then failed to build
//! (`i < 1.5` / `i < false`). The fix resolves the initializer through the
//! same classifier, with a depth cap for const cycles.

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e2009(src: &str) {
    let cs = codes(&check(src));
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009, got {cs:?} for:\n{src}"
    );
}

fn accepts_without_e2009(src: &str) {
    let cs = codes(&check(src));
    assert!(
        !cs.iter().any(|c| c == "HYB-E2009"),
        "unexpected HYB-E2009, got {cs:?} for:\n{src}"
    );
}

fn range_with(files: &str, hi: &str) -> String {
    format!("{files}\nfn main() {{\n    for i in 0..{hi} {{\n        _ = i\n    }}\n}}\n")
}

#[test]
fn compound_float_const_bound_is_rejected() {
    rejects_with_e2009(&range_with("const hi = 1.5 + 0.5", "hi"));
}

#[test]
fn parenthesized_float_const_bound_is_rejected() {
    rejects_with_e2009(&range_with("const hi = (1.5 + 0.5)", "hi"));
}

#[test]
fn negated_float_const_bound_is_rejected() {
    rejects_with_e2009(&range_with("const hi = -1.5", "hi"));
}

#[test]
fn compound_bool_const_bound_is_rejected() {
    rejects_with_e2009(&range_with("const flag = !false", "flag"));
}

#[test]
fn chained_const_bound_is_rejected() {
    rejects_with_e2009(&range_with("const base = 1.5\nconst hi = base", "hi"));
}

#[test]
fn compound_string_const_bound_is_rejected() {
    rejects_with_e2009(&range_with("const label = \"a\" + \"b\"", "label"));
}

#[test]
fn compound_integer_const_bound_stays_accepted() {
    accepts_without_e2009(&range_with("const n = 1 + 2", "n"));
    accepts_without_e2009(&range_with("const n = 2 * 3", "n"));
    accepts_without_e2009(&range_with("const base = 3\nconst n = base + 1", "n"));
}

#[test]
fn const_cycle_terminates_without_e2009() {
    accepts_without_e2009(&range_with("const a = b\nconst b = a", "a"));
}

#[test]
fn chained_const_bound_isolated_from_local_shadow_and_typed_const_chain_rejects() {
    rejects_with_e2009(
        "const a = 1.5\nconst b = a\nfn main() {\n    a := 1\n    _ = a\n    for i in 0..b {\n        _ = i\n    }\n}\n",
    );
    accepts_without_e2009(
        "const a = 5\nconst b = a\nfn main() {\n    a := 1.5\n    _ = a\n    for i in 0..b {\n        _ = i\n    }\n}\n",
    );
    rejects_with_e2009(
        "const a u32 = 1\nconst b = a\nfn main() {\n    var hi u64 = 10\n    for i in b..hi {\n        _ = i\n    }\n}\n",
    );
    rejects_with_e2009(
        "const b = u32(1)\nfn main() {\n    var hi u64 = 10\n    for i in b..hi {\n        _ = i\n    }\n}\n",
    );
    rejects_with_e2009(
        "type my_u32 = u32\nfn main() {\n    var hi u64 = 10\n    for i in my_u32(0)..hi {\n        _ = i\n    }\n}\n",
    );
    rejects_with_e2009(
        "enum set { a }\nfn f(x set) {\n    for v in x {\n        _ = v\n    }\n}\nfn main() {}\n",
    );
    let cs = codes(&check("fn f[set](x set) {\n    for v in x {\n        _ = v\n    }\n}\nfn main() {}\n"));
    assert!(cs.iter().any(|c| c == "HYB-E3005"), "expected HYB-E3005 for type-param set, got {cs:?}");
}
