//! const_div_zero.rs — regression for issue #149: division/modulo by a
//! constant `0` used to pass `hyb check` (0 diagnostics) and then fail
//! `go vet` with `invalid operation: division by zero`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

const DIV_BY_ZERO: &str = "\
fn main() {
    x := 5
    y := x / 0
    fmt.println(\"{y}\")
}
";

const MOD_BY_ZERO: &str = "\
fn main() {
    x := 5
    y := x % 0
    fmt.println(\"{y}\")
}
";

const DIV_ASSIGN_BY_ZERO: &str = "\
fn main() {
    x := 5
    x /= 0
    fmt.println(\"{x}\")
}
";

const MOD_ASSIGN_BY_ZERO: &str = "\
fn main() {
    x := 5
    x %= 0
    fmt.println(\"{x}\")
}
";

const DIV_BY_NEG_ZERO: &str = "\
fn main() {
    x := 5
    y := x / -0
    fmt.println(\"{y}\")
}
";

const MOD_BY_NEG_ZERO: &str = "\
fn main() {
    x := 5
    y := x % -0
    fmt.println(\"{y}\")
}
";

const DIV_ASSIGN_BY_NEG_ZERO: &str = "\
fn main() {
    x := 5
    x /= -0
    fmt.println(\"{x}\")
}
";

const MOD_ASSIGN_BY_NEG_ZERO: &str = "\
fn main() {
    x := 5
    x %= -0
    fmt.println(\"{x}\")
}
";

const PAREN_NEG_ZERO: &str = "\
fn main() {
    x := 5
    y := x / (-0)
    z := x % -(0)
    fmt.println(\"{y} {z}\")
}
";

const DOUBLE_NEG_ZERO: &str = "\
fn main() {
    x := 5
    y := x / -(-0)
    fmt.println(\"{y}\")
}
";

const NONCONST_ZERO_OK: &str = "\
fn main() {
    x := 5
    z := 0
    y := x / z
    fmt.println(\"{y}\")
}
";

const NONZERO_OK: &str = "\
fn main() {
    x := 5
    y := x / 2
    z := x % 3
    fmt.println(\"{y} {z}\")
}
";

const NEG_NONZERO_OK: &str = "\
fn main() {
    x := 5
    y := x / -2
    z := x % -3
    x /= -5
    x %= -7
    fmt.println(\"{y} {z} {x}\")
}
";

#[test]
fn literal_div_by_zero_is_rejected() {
    let r = check(DIV_BY_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn literal_mod_by_zero_is_rejected() {
    let r = check(MOD_BY_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn literal_div_assign_by_zero_is_rejected() {
    let r = check(DIV_ASSIGN_BY_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn literal_mod_assign_by_zero_is_rejected() {
    let r = check(MOD_ASSIGN_BY_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn literal_div_by_neg_zero_is_rejected() {
    let r = check(DIV_BY_NEG_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn literal_mod_by_neg_zero_is_rejected() {
    let r = check(MOD_BY_NEG_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn literal_div_assign_by_neg_zero_is_rejected() {
    let r = check(DIV_ASSIGN_BY_NEG_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn literal_mod_assign_by_neg_zero_is_rejected() {
    let r = check(MOD_ASSIGN_BY_NEG_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn parenthesized_neg_zero_is_rejected() {
    let r = check(PAREN_NEG_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn double_neg_zero_is_rejected() {
    let r = check(DOUBLE_NEG_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nonconst_zero_divisor_stays_accepted() {
    // Run-time panic path — out of scope for #149's first cut.
    let r = check(NONCONST_ZERO_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6001"),
        "non-constant zero must not fire HYB-E6001, got {:?}",
        cs
    );
}

#[test]
fn nonzero_divisor_stays_accepted() {
    let r = check(NONZERO_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6001"),
        "nonzero divisor must stay clean, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn negative_nonzero_divisor_stays_accepted() {
    let r = check(NEG_NONZERO_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6001"),
        "negative nonzero divisor must stay clean, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const DIV_BY_FOLDED_ZERO: &str = "\
fn main() {
    x := 5
    y := x / (1 - 1)
    _ = y
}
";

const MOD_BY_FOLDED_ZERO: &str = "\
fn main() {
    x := 5
    y := x % (2 - 2)
    _ = y
}
";

const DIV_ASSIGN_BY_FOLDED_ZERO: &str = "\
fn main() {
    x := 5
    x /= (0 * 5)
    _ = x
}
";

const MOD_ASSIGN_BY_FOLDED_ZERO: &str = "\
fn main() {
    x := 5
    x %= (0 + 0)
    _ = x
}
";

const FOLDED_NONZERO_OK: &str = "\
fn main() {
    x := 5
    a := x / (1 - 0)
    b := x / (2 - 1)
    c := x / (0 * 5 + 1)
    _ = a
    _ = b
    _ = c
}
";

const LEGACY_OCTAL_NONZERO_OK: &str = "\
fn main() {
    x := 5
    y := x / (010 - 10)
    _ = y
}
";

const LEGACY_OCTAL_ZERO_REJECT: &str = "\
fn main() {
    x := 5
    y := x / (010 - 8)
    _ = y
}
";

const EXPLICIT_OCTAL_NONZERO_OK: &str = "\
fn main() {
    x := 5
    y := x / (0o10 - 10)
    _ = y
}
";

const EXPLICIT_OCTAL_ZERO_REJECT: &str = "\
fn main() {
    x := 5
    y := x / (0o10 - 8)
    _ = y
}
";

const HEX_FOLDED_ZERO_REJECT: &str = "\
fn main() {
    x := 5
    y := x / (0x10 - 16)
    _ = y
}
";

const BIN_FOLDED_ZERO_REJECT: &str = "\
fn main() {
    x := 5
    y := x / (0b10 - 2)
    _ = y
}
";

const BITWISE_FOLDED_ZERO_REJECT: &str = "\
fn main() {
    x := 5
    y := x / (1 & 0)
    z := x / (1 ^ 1)
    _ = y
    _ = z
}
";

const FILE_CONST_DIV_BY_FOLDED_ZERO: &str = "\
const c_val = 1 / (1 - 1)

fn main() {
    _ = c_val
}
";

const FILE_CONST_DIV_BY_LITERAL_ZERO: &str = "\
const c_val = 1 / 0

fn main() {
    _ = c_val
}
";

#[test]
fn folded_div_by_zero_is_rejected() {
    let r = check(DIV_BY_FOLDED_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn folded_mod_by_zero_is_rejected() {
    let r = check(MOD_BY_FOLDED_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn folded_div_assign_by_zero_is_rejected() {
    let r = check(DIV_ASSIGN_BY_FOLDED_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn folded_mod_assign_by_zero_is_rejected() {
    let r = check(MOD_ASSIGN_BY_FOLDED_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn folded_nonzero_divisor_stays_accepted() {
    let r = check(FOLDED_NONZERO_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6001"),
        "folded nonzero divisor must stay clean, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn legacy_octal_nonzero_divisor_stays_accepted() {
    let r = check(LEGACY_OCTAL_NONZERO_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6001"),
        "legacy octal 010 - 10 is -2 in Go, must stay accepted, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn legacy_octal_folded_zero_is_rejected() {
    let r = check(LEGACY_OCTAL_ZERO_REJECT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "legacy octal 010 - 8 is zero in Go, expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn explicit_octal_nonzero_divisor_stays_accepted() {
    let r = check(EXPLICIT_OCTAL_NONZERO_OK);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6001"),
        "explicit octal 0o10 - 10 is -2 in Go, must stay accepted, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn explicit_octal_folded_zero_is_rejected() {
    let r = check(EXPLICIT_OCTAL_ZERO_REJECT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "explicit octal 0o10 - 8 is zero in Go, expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn hex_folded_zero_is_rejected() {
    let r = check(HEX_FOLDED_ZERO_REJECT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "0x10 - 16 is zero, expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_folded_zero_is_rejected() {
    let r = check(BIN_FOLDED_ZERO_REJECT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "0b10 - 2 is zero, expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn bitwise_folded_zero_is_rejected() {
    let r = check(BITWISE_FOLDED_ZERO_REJECT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "1 & 0 and 1 ^ 1 are zero, expected HYB-E6001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_const_div_by_folded_zero_is_rejected() {
    let r = check(FILE_CONST_DIV_BY_FOLDED_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "file-level const with folded zero divisor must be rejected, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_const_div_by_literal_zero_is_rejected() {
    let r = check(FILE_CONST_DIV_BY_LITERAL_ZERO);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "file-level const with literal zero divisor must be rejected, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
