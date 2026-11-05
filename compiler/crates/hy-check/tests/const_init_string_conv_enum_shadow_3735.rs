//! Regression tests for #3735: const_init.rs misses non-constant string
//! conversion arguments and enum-shadowed const type ascriptions (HYB-E6003).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

#[test]
fn rejects_non_const_string_conversion_arguments() {
    // string(literal) is accepted
    rejects_with(
        "fn get_str() string { return \"x\" }\n\
        const a = len(string(get_str()))\n\
        fn main() {}\n",
        "HYB-E6003",
    );
    
    rejects_with(
        "fn main() {\n\
            let x = 10\n\
            const e = len(string(x))\n\
        }\n",
        "HYB-E6003",
    );
}

#[test]
fn rejects_const_shadowed_string_conversion_target() {
    rejects_with(
        "const string = 1\n\
        const b string = \"ok\"\n\
        const g = len(string(b))\n\
        fn main() {}\n",
        "HYB-E6003",
    );
}

#[test]
fn rejects_enum_shadowed_const_type_ascriptions() {
    rejects_with(
        "enum int { variant_a }\n\
        const h int = 1\n\
        fn main() {}\n",
        "HYB-E6003",
    );
    
    rejects_with(
        "enum string { s_variant }\n\
        const j string = \"abc\"\n\
        fn main() {}\n",
        "HYB-E6003",
    );
}

#[test]
fn rejects_type_param_shadowed_const_type_ascriptions() {
    rejects_with(
        "struct box[int] { v int }\n\
        const m int = 99\n\
        fn main() {}\n",
        "HYB-E6003",
    );
}
