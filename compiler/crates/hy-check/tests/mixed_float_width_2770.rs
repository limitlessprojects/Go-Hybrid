//! mixed_float_width_2770.rs — issue #2770 (Row 13 BUILDABLE).
//!
//! `float32` and `float64` both collapse to `OpKind::Float`, so `f + g`,
//! `f < g`, `f == g`, `var h float64 = f`, `g = f` and `half(f)` between two
//! typed floats of different widths passed `hyb check` and failed `go build`
//! (`mismatched types float32 and float64`). Untyped float constants still adapt.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn mixed_float_widths_reject_e1002() {
    for (name, src) in [
        (
            "f + g",
            "fn main() {\n    var f f32 = 1.0\n    var g f64 = 2.0\n    _ = f + g\n}\n",
        ),
        (
            "f < g",
            "fn main() {\n    var f f32 = 1.0\n    var g f64 = 2.0\n    _ = f < g\n}\n",
        ),
        (
            "f == g",
            "fn main() {\n    var f f32 = 1.0\n    var g f64 = 2.0\n    _ = f == g\n}\n",
        ),
        (
            "var h f64 = f",
            "fn main() {\n    var f f32 = 1.0\n    var h f64 = f\n    _ = h\n}\n",
        ),
        (
            "g = f",
            "fn main() {\n    var f f32 = 1.0\n    var g f64 = 2.0\n    g = f\n    _ = g\n}\n",
        ),
        (
            "half(f)",
            "fn half(x f64) f64 {\n    return x\n}\nfn main() {\n    var f f32 = 1.0\n    _ = half(f)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn untyped_constants_and_same_width_stay_accepted() {
    for (name, src) in [
        (
            "f64 + 1.5",
            "fn main() {\n    var f f64 = 1.0\n    _ = f + 1.5\n}\n",
        ),
        (
            "f32 + 1.5",
            "fn main() {\n    var f f32 = 1.0\n    _ = f + 1.5\n}\n",
        ),
        (
            "f32 + f32",
            "fn main() {\n    var a f32 = 1.0\n    var b f32 = 2.0\n    _ = a + b\n}\n",
        ),
        (
            "byte + uint8 aliases",
            "fn main() {\n    var b byte = 1\n    var u uint8 = 2\n    _ = b + u\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
