//! Regression tests for constant division/modulo by zero (`HYB-E6001`) across
//! recursive `len(...)` / integer-conversion / unary-negation divisors,
//! hexadecimal and conversion-based float constant expressions, and nested
//! closure / match-arm shadowing scopes.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn e6001(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E6001")
        .count()
}

#[test]
fn negated_converted_and_concatenated_len_zero_divisors_reject_e6001() {
    for (label, src) in [
        ("negated len(\"\") div", "fn main() {\n    x := 10 / -len(\"\")\n    _ = x\n}\n"),
        ("negated len(\"abc\")-3 mod", "fn main() {\n    x := 10 % -(len(\"abc\") - 3)\n    _ = x\n}\n"),
        ("div-assign negated len(\"\")", "fn main() {\n    x := 10\n    x /= -len(\"\")\n    _ = x\n}\n"),
        ("int(len(\"\")) div", "fn main() {\n    x := 10 / int(len(\"\"))\n    _ = x\n}\n"),
        ("int64(len(\"abc\")-3) mod", "fn main() {\n    x := 10 % int64(len(\"abc\") - 3)\n    _ = x\n}\n"),
        ("aliased my_int(0) div", "type my_int = int\nfn main() {\n    x := 10 / my_int(0)\n    _ = x\n}\n"),
        ("aliased my_int(len)-3 div", "type my_int = int\nfn main() {\n    x := 10 / (my_int(len(\"abc\")) - 3)\n    _ = x\n}\n"),
        ("len of concat strings", "fn main() {\n    x := 10 / (len(\"a\" + \"bc\") - 3)\n    _ = x\n}\n"),
        ("len of chained const string", "const a = \"abc\"\nconst b = a\nfn main() {\n    x := 10 / (len(b) - 3)\n    _ = x\n}\n"),
        ("len of string(...) conv", "type my_str = string\nfn main() {\n    x := 10 / (len(my_str(\"abc\")) - 3)\n    _ = x\n}\n"),
    ] {
        let r = check(src);
        assert_eq!(e6001(&r), 1, "{label} must reject with HYB-E6001: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn float_conversions_and_compound_float_zero_divisors_reject_e6001() {
    for (label, src) in [
        ("float64(1) / 0.0", "fn main() {\n    _ = float64(1) / 0.0\n}\n"),
        ("f64(2) / f64(0)", "fn main() {\n    _ = f64(2) / f64(0)\n}\n"),
        ("1.0 / -float64(0)", "fn main() {\n    _ = 1.0 / -float64(0)\n}\n"),
        ("1.0 / (float64(3)-float64(3))", "fn main() {\n    _ = 1.0 / (float64(3) - float64(3))\n}\n"),
        ("const fz = float64(0)", "const fz = float64(0)\nfn main() {\n    _ = 1.0 / fz\n}\n"),
        ("aliased my_f64(0)", "type my_f64 = float64\nfn main() {\n    _ = 1.0 / my_f64(0)\n}\n"),
        ("1.0 / float64(len(\"\"))", "fn main() {\n    _ = 1.0 / float64(len(\"\"))\n}\n"),
        ("1.0 / f64(len(\"abc\")-3)", "fn main() {\n    _ = 1.0 / f64(len(\"abc\") - 3)\n}\n"),
        ("const num = float64(5) / 0.0", "const num = float64(5)\nfn main() {\n    _ = num / 0.0\n}\n"),
        ("f32 conversion subtraction divisor", "const num = 1.0\nfn main() {\n    _ = (num + float64(1)) / (f32(2) - f32(2))\n}\n"),
    ] {
        let r = check(src);
        assert_eq!(e6001(&r), 1, "{label} must reject with HYB-E6001: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn shadowed_len_and_closure_match_or_nested_fnlit_stay_accepted() {
    for (label, src) in [
        (
            "local string variable shadowing file const in len(s)",
            "const s = \"abc\"\nfn main() {\n    s := \"hello\"\n    x := 10 / (len(s) - 3)\n    _ = x\n}\n",
        ),
        (
            "user-defined len function shadowing builtin len",
            "fn len(s string) int {\n    _ = s\n    return 5\n}\nfn main() {\n    x := 10 / len(\"\")\n    _ = x\n}\n",
        ),
        (
            "outer closure param shadowing file const across nested inner closure",
            "const zero = 0\nfn main() {\n    f := fn(zero int) int {\n        g := fn() int { return 1 }\n        _ = g()\n        return 10 / zero\n    }\n    _ = f(2)\n}\n",
        ),
        (
            "closure match arm binding shadowing file const zero",
            "const zero = 0\nfn main() {\n    opt := some(2)\n    f := fn() int {\n        match opt {\n            some(zero) => { return 10 / zero }\n            none => { return 0 }\n        }\n    }\n    _ = f()\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(e6001(&r), 0, "{label} must not emit HYB-E6001: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Accept);
    }
}
