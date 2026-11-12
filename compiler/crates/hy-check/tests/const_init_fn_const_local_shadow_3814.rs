//! Regression tests: `const_init.rs` must reject `const` type ascriptions and
//! alias-resolved conversion targets shadowed by functions, constants, or local
//! bindings with `HYB-E6003`, while keeping local variables from shadowing
//! file-level type alias targets.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_e6003(src: &str, min_count: usize) {
    let r = check(src);
    let count = r.diagnostics.iter().filter(|d| d.code == "HYB-E6003").count();
    assert!(
        count >= min_count,
        "expected >= {min_count} HYB-E6003, got {count} ({:?}) for:\n{src}",
        r.diagnostics,
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn fn_const_and_local_shadowed_const_ascriptions_and_alias_targets_reject_e6003() {
    assert_e6003("fn int() {}\nconst x int = 2\nfn main() {}\n", 1);
    assert_e6003("const int = 1\nconst x int = 2\nfn main() {}\n", 1);
    assert_e6003(
        "fn main() {\n    let int = 1\n    const x int = 2\n    _ = int\n    _ = x\n}\n",
        1,
    );
    assert_e6003(
        "fn main() {\n    let string = \"a\"\n    const s string = \"b\"\n    const n = len(s)\n    _ = string\n    _ = n\n}\n",
        2,
    );
    assert_e6003(
        "const int8 = 1\ntype my_i8 = int8\nconst x = my_i8(42)\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "fn string() {}\ntype my_str = string\nconst n = len(my_str(\"abc\"))\nfn main() {}\n",
        1,
    );
}

#[test]
fn local_binding_does_not_shadow_file_level_alias_target() {
    assert_accept(
        "type my_i8 = int8\nfn main() {\n    let int8 = 1\n    const x = my_i8(42)\n    _ = int8\n    _ = x\n}\n",
    );
}
