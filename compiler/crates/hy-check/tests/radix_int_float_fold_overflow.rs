//! radix_int_float_fold_overflow.rs — regression tests for folding hexadecimal,
//! binary, and octal integer literals >= 2^127 into float constant contexts
//! (`const_float_fold_with`) and explicit float conversions (`huge_int_as_f64`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test_radix_float_overflow.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_with(src: &str, code: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(
        codes.iter().any(|c| c == code),
        "expected {code} for:\n{src}\ngot: {codes:?}"
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn radix_int_past_f32_max_rejected_in_var_and_const_init() {
    // 2^128 > f32::MAX in hex, uppercase hex, binary, and octal
    let hex_2_128 = "0x100000000000000000000000000000000";
    let hex_upper_2_132 = "0X1000000000000000000000000000000000";
    let bin_2_128 = format!("0b1{}", "0".repeat(128));
    let oct_2_128 = format!("0o4{}", "0".repeat(42));

    for lit in [hex_2_128, hex_upper_2_132, &bin_2_128, &oct_2_128] {
        let var_src = format!("fn main() {{\n    var x f32 = {lit}\n    fmt.println(x)\n}}\n");
        assert_rejects_with(&var_src, "HYB-E6002");

        let const_src = format!("const k f32 = {lit}\nfn main() {{\n    fmt.println(k)\n}}\n");
        assert_rejects_with(&const_src, "HYB-E6002");
    }
}

#[test]
fn radix_int_past_f32_max_rejected_in_assign_return_call_and_exprs() {
    let hex_2_128 = "0x100000000000000000000000000000000";

    let assign_src = format!(
        "fn main() {{\n    var x f32 = 1.0\n    x = {hex_2_128}\n    fmt.println(x)\n}}\n"
    );
    assert_rejects_with(&assign_src, "HYB-E6002");

    let ret_src = format!(
        "fn bad() f32 {{\n    return {hex_2_128}\n}}\nfn main() {{\n    fmt.println(bad())\n}}\n"
    );
    assert_rejects_with(&ret_src, "HYB-E6002");

    let call_src = format!(
        "fn take(v f32) f32 {{\n    return v\n}}\nfn main() {{\n    fmt.println(take({hex_2_128}))\n}}\n"
    );
    assert_rejects_with(&call_src, "HYB-E6002");

    let neg_src = format!("fn main() {{\n    var z f32 = -{hex_2_128}\n    fmt.println(z)\n}}\n");
    assert_rejects_with(&neg_src, "HYB-E6002");

    let add_src =
        format!("fn main() {{\n    var w f32 = {hex_2_128} + 1.0\n    fmt.println(w)\n}}\n");
    assert_rejects_with(&add_src, "HYB-E6002");
}

#[test]
fn radix_int_past_f64_max_rejected_in_f64_init_and_conversion() {
    // 0x1 followed by 256 hex zeros = 16^256 = 2^1024 > f64::MAX
    let hex_2_1024 = format!("0x1{}", "0".repeat(256));

    let init_src = format!("fn main() {{\n    var y f64 = {hex_2_1024}\n    fmt.println(y)\n}}\n");
    assert_rejects_with(&init_src, "HYB-E6002");

    let conv_f64 = format!("fn main() {{\n    y := f64({hex_2_1024})\n    fmt.println(y)\n}}\n");
    assert_rejects_with(&conv_f64, "HYB-E6006");

    let conv_f32 =
        "fn main() {\n    x := f32(0x100000000000000000000000000000000)\n    fmt.println(x)\n}\n";
    assert_rejects_with(conv_f32, "HYB-E6006");
}

#[test]
fn in_range_radix_int_accepted_for_f64_and_f32() {
    // 2^128 and 2^132 fit comfortably in f64 (max ~1.7977e308)
    let ok_f64_init =
        "fn main() {\n    var ok f64 = 0x100000000000000000000000000000000\n    fmt.println(ok)\n}\n";
    assert_accepts(ok_f64_init);

    let ok_f64_conv =
        "fn main() {\n    ok1 := f64(0x100000000000000000000000000000000)\n    ok2 := float64(0x1000000000000000000000000000000000)\n    fmt.println(ok1, ok2)\n}\n";
    assert_accepts(ok_f64_conv);

    // f32::MAX = (2^24 - 1) * 2^104 = 0xffffff00000000000000000000000000
    let ok_f32_max =
        "fn main() {\n    var ok f32 = 0xffffff00000000000000000000000000\n    fmt.println(ok)\n}\n";
    assert_accepts(ok_f32_max);
}
