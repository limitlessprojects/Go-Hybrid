//! option_value_conversion.rs — an `option[T]` value lowers to the generic
//! prelude struct `option[T]`, which Go converts to nothing but itself or an
//! interface. `u8(op)`, `float64(op)`, `string(op)`, `int(f())` (f returns
//! option[int]) emit Go that fails `go build` (`cannot convert op (variable
//! of struct type option[int]) to type uint8`), so `hyb check` must reject
//! them with HYB-E1002 (GUARANTEES row 13, BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.severity == hy_diag::Severity::Error && d.code == "HYB-E1002")
}

#[test]
fn option_value_converted_to_scalar_rejects_e1002() {
    let mut cases: Vec<String> = ["u8", "int", "i64", "f32", "float64", "rune", "string"]
        .iter()
        .map(|t| format!("fn main() {{\n    op := some(1)\n    z := {}(op)\n    fmt.println(z)\n}}\n", t))
        .collect();
    cases.push("fn f() option[int] { return some(2) }\nfn main() {\n    z := float64(f())\n    fmt.println(z)\n}\n".into());
    cases.push("fn main() {\n    var op option[string] = some(\"a\")\n    z := string(op)\n    fmt.println(z)\n}\n".into());
    cases.push("fn take(n int) { fmt.println(n) }\nfn main() {\n    op := some(1)\n    take(int(op))\n}\n".into());
    for src in &cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn option_payload_conversions_still_accept() {
    let cases = [
        "fn main() {\n    op := some(1)\n    z := u8(op.unwrap())\n    fmt.println(z)\n}\n",
        "fn main() {\n    op := some(1)\n    z := float64(op.or(3))\n    fmt.println(z)\n}\n",
        "fn main() {\n    op := some(1)\n    z := any(op)\n    fmt.println(z)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected acceptance for:\n{}\n{:?}", src, r.diagnostics);
    }
}
