//! const_conversion_matrix_2853.rs — issue #2853 (Row 13 BUILDABLE).
//!
//! `const_overflow.rs` decides `HYB-E6006` from a literal-only view of the
//! conversion operand and only for the bare callee spellings. Three classes of
//! accepted-but-unbuildable program fell through:
//!
//! 1. the callee: a parenthesized conversion `(int8)(2.5)` and a `type` alias
//!    of a predeclared type (`type my_i8 = int8; my_i8(2.5)`) were not
//!    recognised as conversions at all;
//! 2. the operand: a named constant (`const c = 300; int8(c)`) and a folded
//!    constant expression (`int8(1.5 + 1.0)`, `float32(1e30 * 1e20)`) carried
//!    no literal for `conv_const` to read;
//! 3. the element of a slice conversion: a `type` alias of `byte`/`rune` was
//!    not resolved, so the legal `[]my_byte("abc")` was rejected while a
//!    declared `fn int8(…)` shadowing the predeclared name was not.
//!
//! The reject side is the buildability half of those; the accept side pins the
//! aliased/traced/legal conversions that must keep checking clean.

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

fn rejects_with(src: &str, want: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == want),
        "expected {want} for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_clean(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "must stay accepted for {:?}, got {:?}",
        src,
        cs
    );
}

// ---------------------------------------------------------------- reject:

#[test]
fn parenthesized_and_aliased_conversions_reject_e6006() {
    rejects_with(
        "fn main() {\n    y := (int8)(2.5)\n    fmt.println(\"{y}\")\n}",
        "HYB-E6006",
    );
    rejects_with(
        "type my_i8 = int8\nfn main() {\n    x := my_i8(2.5)\n    fmt.println(\"{x}\")\n}",
        "HYB-E6006",
    );
    rejects_with(
        "type my_f32 = float32\nfn main() {\n    x := my_f32(1e40)\n    fmt.println(\"{x}\")\n}",
        "HYB-E6006",
    );
    rejects_with(
        "type my_str = string\nfn main() {\n    x := my_str(2.5)\n    fmt.println(\"{x}\")\n}",
        "HYB-E6006",
    );
    rejects_with(
        "fn main() {\n    x := (int)(\"abc\")\n    fmt.println(\"{x}\")\n}",
        "HYB-E6006",
    );
    rejects_with(
        "fn main() {\n    x := ([]int)(3)\n    fmt.println(\"{x}\")\n}",
        "HYB-E6006",
    );
}

#[test]
fn named_and_folded_constant_operands_reject_e6006() {
    rejects_with(
        "const s = \"abc\"\nfn main() {\n    z := int(s)\n    fmt.println(\"{z}\")\n}",
        "HYB-E6006",
    );
    rejects_with(
        "fn main() {\n    v := int8(1.5 + 1.0)\n    fmt.println(\"{v}\")\n}",
        "HYB-E6006",
    );
    rejects_with(
        "fn main() {\n    v := float32(1e30 * 1e20)\n    fmt.println(\"{v}\")\n}",
        "HYB-E6006",
    );
}

#[test]
fn len_of_a_string_constant_folds_into_the_overflow_check() {
    // `len("hello") + 125` is 130, outside `int8`.
    rejects_with(
        "fn main() {\n    var y int8 = len(\"hello\") + 125\n    fmt.println(\"{y}\")\n}",
        "HYB-E6002",
    );
}

// ---------------------------------------------------------------- accept:

#[test]
fn aliased_byte_and_rune_slice_conversions_stay_accepted() {
    // Go allows string -> []byte / []rune, and a `type` alias of either is
    // still that conversion (`cannot convert` would be a wrong rejection).
    accepts_clean("type my_byte = byte\nfn main() {\n    v := []my_byte(\"abc\")\n    fmt.println(\"{v}\")\n}");
    accepts_clean("type my_rune = rune\nfn main() {\n    v := []my_rune(\"abc\")\n    fmt.println(\"{v}\")\n}");
    accepts_clean("type bytes = []byte\nfn main() {\n    v := bytes(\"abc\")\n    fmt.println(\"{v}\")\n}");
}

#[test]
fn a_declared_function_shadows_a_predeclared_conversion_name() {
    // `fn int8(x float64) int` makes `int8` an ordinary call, so the constant
    // conversion matrix must not inspect it.
    accepts_clean(
        "fn int8(x float64) int {\n    return 0\n}\n\nfn main() {\n    v := int8(2.5)\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn legal_constant_conversions_stay_accepted() {
    // In range.
    accepts_clean("fn main() {\n    v := int8(2)\n    fmt.println(\"{v}\")\n}");
    // SPEC X4 deliberately folds a SUB-64 constant conversion (`u8(300)` is
    // 44 in the emitted Go), so this is not a rejection.
    accepts_clean("fn main() {\n    v := int8(300)\n    fmt.println(\"{v}\")\n}");
    // `len` of a constant that fits.
    accepts_clean("fn main() {\n    var y int8 = len(\"hi\")\n    fmt.println(\"{y}\")\n}");
    // A literal containing an escape is left unfolded: only the emitted byte
    // count may be folded, and the raw spelling is not that count.
    accepts_clean("fn main() {\n    var y int8 = len(\"a\\nb\")\n    fmt.println(\"{y}\")\n}");
}
