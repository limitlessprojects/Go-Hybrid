//! Soundness and buildable validation tests for `strconv` calls under Row 13 (BUILDABLE):
//! 1. `strconv.int_size` is a package constant, not a function; calls must reject with `HYB-E1002`.
//! 2. `strconv.err_range` and `strconv.err_syntax` are package variable errors; calls must reject with `HYB-E1002`.
//! 3. `strconv.num_error` is a struct type; calls must reject with `HYB-E1002`.
//! 4. Curated `strconv` functions have strict parameter arities:
//!    - 1 arg: atoi, itoa, parse_bool, format_bool, can_backquote, is_graphic, is_print, quote,
//!      quote_rune, quote_rune_to_ascii, quote_rune_to_graphic, quote_to_ascii, quote_to_graphic,
//!      quoted_prefix, unquote.
//!    - 2 args: parse_complex, parse_float, format_int, format_uint, unquote_char, append_bool.
//!    - 3 args: parse_int, parse_uint, append_int, append_uint, append_quote, append_quote_rune,
//!      append_quote_rune_to_ascii, append_quote_rune_to_graphic, append_quote_to_ascii, append_quote_to_graphic.
//!    - 4 args: format_complex, format_float, append_float.
//!    Arity mismatches must reject fail-closed with `HYB-E1002`.
//! 5. Positive controls: legal calls with proper arity accept cleanly.
//!
//! Settlement Wallet (Base L2 USDC): 0x8366bCe3a2D379De7656D7A67015789FaF999f20

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_strconv_non_callable_constants_and_variables_reject() {
    // Constant called as function
    rejects_e1002("import \"strconv\"\nfn main() {\n    _ = strconv.int_size()\n}\n");
    // Errors called as functions
    rejects_e1002("import \"strconv\"\nfn main() {\n    _ = strconv.err_range()\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    _ = strconv.err_syntax()\n}\n");
    // Type called as function
    rejects_e1002("import \"strconv\"\nfn main() {\n    _ = strconv.num_error()\n}\n");
}

#[test]
fn test_strconv_one_arg_functions_arity_reject() {
    // atoi: takes 1
    rejects_e1002("import \"strconv\"\nfn main() {\n    v, err := strconv.atoi()\n    _ = v\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    v, err := strconv.atoi(\"1\", \"2\")\n    _ = v\n    _ = err\n}\n");

    // itoa: takes 1
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.itoa()\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.itoa(1, 2)\n    _ = s\n}\n");

    // parse_bool: takes 1
    rejects_e1002("import \"strconv\"\nfn main() {\n    b, err := strconv.parse_bool()\n    _ = b\n    _ = err\n}\n");

    // format_bool: takes 1
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_bool()\n    _ = s\n}\n");

    // can_backquote: takes 1
    rejects_e1002("import \"strconv\"\nfn main() {\n    b := strconv.can_backquote()\n    _ = b\n}\n");

    // quote: takes 1
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.quote()\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.unquote()\n    _ = s\n}\n");
}

#[test]
fn test_strconv_two_arg_functions_arity_reject() {
    // parse_float: takes 2 (s, bitSize)
    rejects_e1002("import \"strconv\"\nfn main() {\n    f, err := strconv.parse_float(\"3.14\")\n    _ = f\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    f, err := strconv.parse_float(\"3.14\", 64, 1)\n    _ = f\n    _ = err\n}\n");

    // parse_complex: takes 2
    rejects_e1002("import \"strconv\"\nfn main() {\n    c, err := strconv.parse_complex(\"1+2i\")\n    _ = c\n    _ = err\n}\n");

    // format_int: takes 2 (i, base)
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_int(42)\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_int(42, 10, 64)\n    _ = s\n}\n");

    // format_uint: takes 2
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_uint(42)\n    _ = s\n}\n");

    // unquote_char: takes 2
    rejects_e1002("import \"strconv\"\nfn main() {\n    r, mb, tail, err := strconv.unquote_char(\"a\")\n    _ = r\n    _ = mb\n    _ = tail\n    _ = err\n}\n");
}

#[test]
fn test_strconv_three_arg_functions_arity_reject() {
    // parse_int: takes 3 (s, base, bitSize)
    rejects_e1002("import \"strconv\"\nfn main() {\n    i, err := strconv.parse_int(\"42\")\n    _ = i\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    i, err := strconv.parse_int(\"42\", 10)\n    _ = i\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    i, err := strconv.parse_int(\"42\", 10, 64, 0)\n    _ = i\n    _ = err\n}\n");

    // parse_uint: takes 3
    rejects_e1002("import \"strconv\"\nfn main() {\n    u, err := strconv.parse_uint(\"42\", 10)\n    _ = u\n    _ = err\n}\n");

    // append_int: takes 3
    rejects_e1002("import \"strconv\"\nfn main() {\n    b := []byte(\"num:\")\n    b2 := strconv.append_int(b, 42)\n    _ = b2\n}\n");
}

#[test]
fn test_strconv_four_arg_functions_arity_reject() {
    // format_float: takes 4 (f, fmt, prec, bitSize)
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_float(3.14, 'f', 2)\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_float(3.14, 'f', 2, 64, 0)\n    _ = s\n}\n");

    // format_complex: takes 4
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_complex(1.0, 'f', 2)\n    _ = s\n}\n");
}

#[test]
fn test_strconv_positive_controls_accept() {
    let src = r#"import "strconv"

fn main() {
    // Legal calls with exact arity
    v1, err1 := strconv.atoi("123")
    _ = v1
    _ = err1

    s1 := strconv.itoa(42)
    _ = s1

    b1, err2 := strconv.parse_bool("true")
    _ = b1
    _ = err2

    s2 := strconv.format_bool(true)
    _ = s2

    f1, err3 := strconv.parse_float("3.14", 64)
    _ = f1
    _ = err3

    i1, err4 := strconv.parse_int("42", 10, 64)
    _ = i1
    _ = err4

    u1, err5 := strconv.parse_uint("42", 10, 64)
    _ = u1
    _ = err5

    s3 := strconv.format_int(42, 10)
    _ = s3

    s4 := strconv.format_uint(42, 10)
    _ = s4

    s5 := strconv.format_float(3.14159, 'f', 2, 64)
    _ = s5

    bq := strconv.can_backquote("hello")
    _ = bq

    q := strconv.quote("hello")
    _ = q

    uq, err6 := strconv.unquote("\"hello\"")
    _ = uq
    _ = err6
}
"#;
    accepts(src);
}
