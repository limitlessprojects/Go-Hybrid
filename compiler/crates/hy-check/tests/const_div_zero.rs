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
