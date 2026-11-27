use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast =
        parse_file("inline.hy", src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_single_e6002(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
    assert_eq!(
        codes,
        vec!["HYB-E6002".to_string()],
        "expected single HYB-E6002 for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_consts_and_shrunk_shifts_reaching_2_pow_512_reject() {
    for src in [
        "const huge = 1 << 512\nfn main() {}\n",
        "const huge = 2 << 511\nfn main() {}\n",
        "const huge = -1 << 512\nfn main() {}\n",
        "const a = 1 << 300\nconst b = a << 212\nfn main() {}\n",
        "fn main() {\n    const local_huge = 1 << 512\n    _ = local_huge\n}\n",
        "fn main() {\n    x := (1 << 512) & 0\n    _ = x\n}\n",
        "fn main() {\n    x := (1 << 600) >> 599\n    _ = x\n}\n",
        "fn main() {\n    x := (1 << 512) / (1 << 511)\n    _ = x\n}\n",
        "fn main() {\n    x := u8((1 << 512) & 1)\n    _ = x\n}\n",
        "fn main() {\n    b := ((1 << 512) & 0) == 0\n    _ = b\n}\n",
    ] {
        assert_rejects_single_e6002(src);
    }
}

#[test]
fn shifts_below_2_pow_512_zero_lhs_and_runtime_shifts_stay_accepted() {
    for src in [
        "const max_pow = 1 << 511\nconst max_two = 2 << 510\nconst neg_pow = -1 << 511\nconst a = 1 << 300\nconst b = a << 211\nconst zero_shift = 0 << 1000\nfn main() {}\n",
        "fn main() {\n    x := (1 << 511) >> 510\n    y := (1 << 511) & 1\n    _ = x\n    _ = y\n}\n",
        "const k = 1\nfn shift_param(k int) int {\n    return k << 512\n}\nfn main() {\n    r := 1\n    x := r << 512\n    y := shift_param(1)\n    _ = x\n    _ = y\n}\n",
    ] {
        assert_accepts(src);
    }
}
