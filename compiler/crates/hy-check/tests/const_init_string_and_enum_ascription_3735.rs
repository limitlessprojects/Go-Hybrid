//! Regression tests for #3735: `const_init.rs` must reject enum- and type-
//! parameter-shadowed `const` type ascriptions (both direct and via type
//! aliases) and `const`-shadowed `string` conversions with `HYB-E6003`.

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
fn enum_and_struct_shadowed_const_ascriptions_reject_e6003() {
    assert_e6003("enum int { a }\nconst x int = 1\nfn main() {}\n", 1);
    assert_e6003("enum int { a }\ntype my_int = int\nconst x my_int = 1\nfn main() {}\n", 1);
    assert_e6003(
        "enum string { a }\nconst s string = \"abc\"\nconst n = len(s)\nfn main() {}\n",
        2,
    );
    assert_e6003(
        "enum my_int { a }\ntype my_int = int\nconst x my_int = 1\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "enum my_str { a }\ntype my_str = string\nfn main() {\n    const s my_str = \"abc\"\n    const n = len(s)\n    _ = n\n}\n",
        2,
    );
}

#[test]
fn const_shadowed_string_conv_rejects_and_clean_alias_accepts() {
    assert_e6003("const string = 1\nconst n = len(string(65))\nfn main() {}\n", 1);
    assert_accept("type my_str = string\nconst s my_str = \"abc\"\nconst n = len(string(s))\nfn main() {}\n");
}
