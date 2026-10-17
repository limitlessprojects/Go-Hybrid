//! const_overflow.rs — regression for issue #148: a constant initializer
//! outside a declared fixed-width integer type used to pass `hyb check`
//! (0 diagnostics) and then fail `go vet` / `go build` with
//! `cannot use N as T value (overflows)`.

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

const OVERFLOW_INT8: &str = "\
fn main() {
    var n int8 = 300
    fmt.println(\"{n}\")
}
";

const OVERFLOW_UINT8: &str = "\
fn main() {
    var n uint8 = 256
    fmt.println(\"{n}\")
}
";

const OVERFLOW_BYTE_HEX: &str = "\
fn main() {
    var n byte = 0x100
    fmt.println(\"{n}\")
}
";

const OVERFLOW_NEG_INT8: &str = "\
fn main() {
    var n int8 = -129
    fmt.println(\"{n}\")
}
";

const OVERFLOW_FILE_CONST: &str = "\
const n int8 = 300

fn main() {
    fmt.println(\"{n}\")
}
";

const FITS_INT8: &str = "\
fn main() {
    var n int8 = 127
    var m int8 = -128
    fmt.println(\"{n} {m}\")
}
";

const FITS_BYTE: &str = "\
fn main() {
    var n byte = 255
    fmt.println(\"{n}\")
}
";

const VARIABLE_OPERAND_OK: &str = "\
fn main() {
    x := 300
    y := byte(x)
    fmt.println(\"{y}\")
}
";

const PAREN_OVERFLOW: &str = "\
fn main() {
    var n int8 = (300)
    fmt.println(\"{n}\")
}
";

#[test]
fn int8_constant_300_is_rejected() {
    let r = check(OVERFLOW_INT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for var n int8 = 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn uint8_constant_256_is_rejected() {
    let r = check(OVERFLOW_UINT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn byte_hex_0x100_is_rejected() {
    let r = check(OVERFLOW_BYTE_HEX);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for byte = 0x100, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn int8_neg_129_is_rejected() {
    let r = check(OVERFLOW_NEG_INT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for -129, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_level_const_overflow_is_rejected() {
    let r = check(OVERFLOW_FILE_CONST);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for file-level const, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn paren_wrapped_overflow_is_rejected() {
    let r = check(PAREN_OVERFLOW);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for (300), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn int8_boundary_values_accept() {
    let r = check(FITS_INT8);
    assert!(
        error_codes(&r).is_empty(),
        "127 / -128 must fit int8, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn byte_255_accepts() {
    let r = check(FITS_BYTE);
    assert!(
        error_codes(&r).is_empty(),
        "255 must fit byte, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn variable_conversion_truncation_still_accepts() {
    // Issue #148 control: conversion from a *variable* is the runtime
    // truncation path (SPEC X4) and must stay accepted.
    let r = check(VARIABLE_OPERAND_OK);
    assert!(
        error_codes(&r).is_empty(),
        "variable conversion must stay clean, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const OVERFLOW_UNPARSED_I128_UNTYPED: &str = "\
fn main() {
    x := 999999999999999999999999999999999999999999999
    fmt.println(\"{x}\")
}
";

const OVERFLOW_I64_MAX_PLUS_ONE_UNTYPED: &str = "\
fn main() {
    x := 9223372036854775808
    fmt.println(\"{x}\")
}
";

const OVERFLOW_I64_MIN_MINUS_ONE_UNTYPED: &str = "\
fn main() {
    x := -9223372036854775809
    fmt.println(\"{x}\")
}
";

const OVERFLOW_VAR_UNTYPED: &str = "\
fn main() {
    var x = 9223372036854775808
    fmt.println(\"{x}\")
}
";

const OVERFLOW_LET_UNTYPED: &str = "\
fn main() {
    let x = 9223372036854775808
    fmt.println(\"{x}\")
}
";

const OVERFLOW_UNPARSED_I128_TYPED: &str = "\
fn main() {
    var n int8 = 999999999999999999999999999999999999999999999
    fmt.println(\"{n}\")
}
";

const CONTROLS_DEFAULT_INT_AND_CONST: &str = "\
const file_big = 9223372036854775808

fn main() {
    x := 9223372036854775807
    y := -9223372036854775808
    var u u64 = 18446744073709551615
    const local_big = 9223372036854775808
    const huge_const = 999999999999999999999999999999999999999999999
    fmt.println(\"{x} {y} {u} {local_big} {huge_const} {file_big}\")
}
";

#[test]
fn untyped_literal_past_i128_is_rejected() {
    let r = check(OVERFLOW_UNPARSED_I128_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for huge untyped int literal, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_literal_i64_max_plus_one_is_rejected() {
    let r = check(OVERFLOW_I64_MAX_PLUS_ONE_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x := 9223372036854775808, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_literal_i64_min_minus_one_is_rejected() {
    let r = check(OVERFLOW_I64_MIN_MINUS_ONE_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x := -9223372036854775809, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_var_overflow_is_rejected() {
    let r = check(OVERFLOW_VAR_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for var x = 9223372036854775808, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_let_overflow_is_rejected() {
    let r = check(OVERFLOW_LET_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for let x = 9223372036854775808, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn typed_literal_past_i128_is_rejected() {
    let r = check(OVERFLOW_UNPARSED_I128_TYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for huge literal with declared type, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn controls_default_int_and_untyped_consts_accept() {
    let r = check(CONTROLS_DEFAULT_INT_AND_CONST);
    assert!(
        error_codes(&r).is_empty(),
        "controls must accept cleanly, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn untyped_constant_expression_overflow_is_rejected() {
    let r = check(
        "fn main() {\n    x := 9223372036854775807 + 1\n    fmt.println(\"{x}\")\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "default-int constant expressions must retain the main-branch ConstFold check"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn landed_constfold_regressions_remain_rejected_after_default_int_port() {
    for src in [
        "fn main() {\n    var n int8 = 100 + 100\n    fmt.println(\"{n}\")\n}\n",
        "fn main() {\n    var x int = 2.5\n    fmt.println(\"{x}\")\n}\n",
    ] {
        let r = check(src);
        assert!(
            error_codes(&r).iter().any(|c| c == "HYB-E6002"),
            "landed #224 overflow/truncation behavior must survive #297 integration: {:?}",
            error_codes(&r)
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn folded_default_int_shift_overflows_are_rejected() {
    for src in [
        "fn main() {\n    x := 1 << 63\n    fmt.println(\"{x}\")\n}\n",
        "fn main() {\n    x := 1 << 128\n    fmt.println(\"{x}\")\n}\n",
    ] {
        let r = check(src);
        assert!(
            error_codes(&r).iter().any(|c| c == "HYB-E6002"),
            "folded default-int shift overflow must be HYB-E6002: {:?}",
            error_codes(&r)
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn nondecimal_underscore_literal_past_i128_is_rejected() {
    let r = check(
        "fn main() {\n    x := 0x1_0000_0000_0000_0000_0000_0000_0000_0000\n    fmt.println(\"{x}\")\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "radix parsing must retain >i128 overflow instead of failing open: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
