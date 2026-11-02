use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e6003(src: &str, min_count: usize) {
    let r = check(src);
    let count = r.diagnostics.iter().filter(|d| d.code == "HYB-E6003").count();
    assert!(
        count >= min_count,
        "expected at least {} HYB-E6003 for:\n{}\ngot: {:?}",
        min_count,
        src,
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn local_and_closure_const_non_const_type_ascriptions_rejected_e6003() {
    assert_e6003(
        "fn main() {\n    const x any = 1\n    _ = x\n}\n",
        1,
    );
    assert_e6003(
        "fn main() {\n    const y option[int] = 2\n    _ = y\n}\n",
        1,
    );
    assert_e6003(
        "fn main() {\n    const s option[string] = \"abc\"\n    const n = len(s)\n    _ = n\n}\n",
        2,
    );
    assert_e6003(
        "type opt_str = option[string]\nfn main() {\n    const s opt_str = \"abc\"\n    const n = len(s)\n    _ = n\n}\n",
        2,
    );
    assert_e6003(
        "fn main() {\n    f := fn() {\n        const x any = 1\n        const s option[string] = \"abc\"\n        const n = len(s)\n        _ = x\n        _ = n\n    }\n    f()\n}\n",
        3,
    );
}

#[test]
fn zero_field_struct_and_enum_shadowing_const_conversion_rejected_e6003() {
    assert_e6003(
        "struct int8 {}\nconst x = int8(42)\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "enum int8 { a }\nconst x = int8(42)\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "struct string {}\nconst n = len(string(65))\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "enum string { a }\nconst n = len(string(65))\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "struct int8 {}\ntype my_i8 = int8\nconst x = my_i8(42)\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "enum string { a }\ntype my_str = string\nconst n = len(my_str(65))\nfn main() {}\n",
        1,
    );
}
