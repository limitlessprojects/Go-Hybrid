//! Regression tests for package-level typed `const` declarations (`Decl::Const`)
//! and array-length expressions:
//! 1. Package-level typed `const` declarations must validate initializer type
//!    compatibility (`HYB-E1002`), matching function-local `const` / `var` / `let`
//!    declarations.
//! 2. Package-level `const` initializers and array-length expressions (`[N]T`)
//!    must run constant-conversion checks (`HYB-E6006`), matching statement-level
//!    expressions.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test program must parse");
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn top_level_typed_const_rejects_mismatched_scalar_and_composite_kinds_e1002() {
    let cases = [
        "const a int = \"hello\"\nfn main() {\n    _ = a\n}\n",
        "const b bool = 123\nfn main() {\n    _ = b\n}\n",
        "const s string = 456\nfn main() {\n    _ = s\n}\n",
        "const f float64 = true\nfn main() {\n    _ = f\n}\n",
        "const xs []int = []string{\"a\"}\nfn main() {\n    _ = xs\n}\n",
        "const m map[string]int = map[string]string{\"a\": \"b\"}\nfn main() {\n    _ = m\n}\n",
        "struct point { x int }\nstruct other { x int }\nconst p point = other{x: 1}\nfn main() {\n    _ = p\n}\n",
        "const n int = []int{1, 2}\nfn main() {\n    _ = n\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn top_level_typed_const_rejects_mismatched_numeric_width_and_typed_int_to_float_e1002() {
    let cases = [
        "const x int32 = int64(1)\nfn main() {\n    _ = x\n}\n",
        "const u uint16 = uint8(1)\nfn main() {\n    _ = u\n}\n",
        "const f float64 = float32(1.5)\nfn main() {\n    _ = f\n}\n",
        "const g float64 = int(1)\nfn main() {\n    _ = g\n}\n",
        "const a int32 = 1\nconst b int64 = a\nfn main() {\n    _ = b\n}\n",
        "const a int = 1\nconst f float64 = a\nfn main() {\n    _ = f\n}\n",
        "const a [3]int = [3]int{1, 2, 3}\nconst b [4]int = a\nfn main() {\n    _ = b\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn top_level_const_rejects_invalid_constant_conversions_e6006() {
    let cases = [
        "const x = int8(2.5)\nfn main() {\n    _ = x\n}\n",
        "const x = byte('Ā')\nfn main() {\n    _ = x\n}\n",
        "const x = float32(1e40)\nfn main() {\n    _ = x\n}\n",
        "const x = int64(99999999999999999999)\nfn main() {\n    _ = x\n}\n",
        "const x = uint(-1)\nfn main() {\n    _ = x\n}\n",
        "const x = []int(\"abc\")\nfn main() {\n    _ = x\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E6006"),
            "expected HYB-E6006 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn array_len_rejects_invalid_constant_conversions_e6006() {
    let cases = [
        "fn main() {\n    var a [int8(2.5)]int\n    _ = a\n}\n",
        "fn main() {\n    var a [byte('Ā')]int\n    _ = a\n}\n",
        "fn main() {\n    var a [int64(99999999999999999999)]int\n    _ = a\n}\n",
        "struct s {\n    a [int8(2.5)]int\n}\nfn main() {}\n",
        "type arr = [byte('Ā')]int\nfn main() {}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E6006"),
            "expected HYB-E6006 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn top_level_const_and_array_len_valid_controls_accepted() {
    let src = "\
const a int = 42
const f float64 = 1
const u = 10
const g float64 = u
const b = byte(300)

fn main() {
    var arr [byte(300)]int
    fmt.println(a, f, g, b, len(arr))
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:?}",
        r.diagnostics
    );
}

#[test]
fn inferred_top_level_const_rejects_mismatched_scalar_and_array_len_e1002() {
    let cases = [
        "const a = int32(1)\nconst b int64 = a\nfn main() {\n    _ = b\n}\n",
        "const a int32 = 1\nconst b = a\nconst c int64 = b\nfn main() {\n    _ = c\n}\n",
        "const a = int(1)\nconst f float64 = a\nfn main() {\n    _ = f\n}\n",
        "const a = len(\"abc\")\nconst f float64 = a\nfn main() {\n    _ = f\n}\n",
        "const a = [3]int{1, 2, 3}\nconst b [4]int = a\nfn main() {\n    _ = b\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(codes(&r).iter().any(|c| c == "HYB-E1002"), "got: {:?}", r.diagnostics);
    }
}
