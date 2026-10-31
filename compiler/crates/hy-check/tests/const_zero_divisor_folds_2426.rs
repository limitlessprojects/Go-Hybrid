//! const_zero_divisor_folds_2426.rs — regression for issue #2426: `HYB-E6001`
//! (`arith.rs`) only saw the literal zero divisors, so five more spellings that
//! Go folds to a *constant* zero passed `hyb check` and then failed `go build`
//! with `invalid operation: division by zero`:
//!
//! 1. an integer conversion of a constant float zero (`int(0.0)`,
//!    `uint8(-0.0)`, `i64(1.5 - 1.5)`, `const fz = 0.0; int(fz)`) and the
//!    alias spelling (`type my_int = int; my_int(0)`);
//! 2. `len()` of concatenated / chained constant strings (`len("" + "")`);
//! 3. `len`/`cap` of a zero-length fixed array (`len([0]int{})`, and a binding
//!    declared `[0]int`);
//! 4. `real`/`imag` selected out of a constant `complex(re, im)`;
//! 5. an untyped `0.0` divisor on an integer dividend (`x / 0.0`, `x /= 0.0`),
//!    where Go converts the untyped constant to the dividend's type.
//!
//! The controls the report pins stay accepted: a `float64` dividend with a
//! constant zero divisor (IEEE-754 division at run time), a non-zero array
//! length, and an operand whose length is not a compile-time constant.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e6001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6001"),
        "expected HYB-E6001 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e6001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6001"),
        "must NOT fire HYB-E6001 for {:?}, got {:?}",
        src,
        cs
    );
}

// ---------------------------------------------------------------- reject:

#[test]
fn int_conversion_of_constant_float_zero_rejects() {
    rejects_with_e6001("fn main() {\n    x := 10\n    _ = x / int(0.0)\n}\n");
    rejects_with_e6001("fn main() {\n    x := 10\n    _ = x / uint8(-0.0)\n}\n");
    rejects_with_e6001("fn main() {\n    x := 10\n    _ = x / i64(1.5 - 1.5)\n}\n");
}

#[test]
fn int_conversion_of_named_float_zero_const_rejects() {
    rejects_with_e6001("const fz = 0.0\nfn main() {\n    x := 10\n    _ = x / int(fz)\n}\n");
}

#[test]
fn int_conversion_through_type_alias_rejects() {
    rejects_with_e6001(
        "type my_int = int\nfn main() {\n    x := 10\n    _ = x / my_int(0)\n}\n",
    );
}

#[test]
fn len_of_concatenated_empty_strings_rejects() {
    rejects_with_e6001("fn main() {\n    x := 10\n    _ = x / len(\"\" + \"\")\n}\n");
    rejects_with_e6001(
        "const e = \"\"\nfn main() {\n    x := 10\n    _ = x / len(e + \"\")\n}\n",
    );
}

#[test]
fn len_of_zero_length_array_literal_rejects() {
    rejects_with_e6001("fn main() {\n    x := 10\n    _ = x / len([0]int{})\n}\n");
    rejects_with_e6001("fn main() {\n    x := 10\n    _ = x / cap([0]int{})\n}\n");
}

#[test]
fn len_of_zero_length_array_binding_rejects() {
    rejects_with_e6001(
        "fn main() {\n    x := 10\n    var a [0]int\n    _ = x / len(a)\n}\n",
    );
}

#[test]
fn real_and_imag_of_complex_zero_component_rejects() {
    rejects_with_e6001(
        "fn main() {\n    x := 10\n    _ = x / int(real(complex(0, 5)))\n}\n",
    );
    rejects_with_e6001(
        "fn main() {\n    x := 10\n    _ = x / int(imag(complex(5, 0)))\n}\n",
    );
}

#[test]
fn untyped_float_zero_on_integer_dividend_rejects() {
    rejects_with_e6001("fn main() {\n    x := 10\n    _ = x / 0.0\n}\n");
    rejects_with_e6001("fn main() {\n    x := 10\n    x /= 0.0\n    _ = x\n}\n");
}

// ---------------------------------------------------------------- accept:

#[test]
fn float_dividend_keeps_ieee_division_accepted() {
    accepts_without_e6001(
        "fn main() {\n    var f float64 = 1.0\n    _ = f / 0.0\n}\n",
    );
}

#[test]
fn non_zero_folds_stay_accepted() {
    accepts_without_e6001("fn main() {\n    x := 10\n    _ = x / int(1.5)\n}\n");
    accepts_without_e6001("fn main() {\n    x := 10\n    _ = x / len(\"ab\" + \"c\")\n}\n");
    accepts_without_e6001("fn main() {\n    x := 10\n    _ = x / len([2]int{})\n}\n");
    accepts_without_e6001("fn main() {\n    x := 10\n    _ = x / real(complex(1, 5))\n}\n");
    accepts_without_e6001("fn main() {\n    x := 10\n    _ = x / len(make([]int, 0))\n}\n");
}
