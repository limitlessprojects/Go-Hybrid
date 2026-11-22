//! Soundness tests for `const_overflow.rs`:
//! 1. Local `const` float bindings in typed/untyped declarations, assignments, and `any`/print slots (`HYB-E6002`).
//! 2. Folded float constant expressions and `const` float identifiers assigned to integer types (`HYB-E6002`).
//! 3. Folded constant expressions and `const` identifiers in type conversion calls (`HYB-E6006`).
//! 4. Shadowed conversion target identifiers (`fn int8(...)`, `fn u8(...)`) must not be falsely rejected or folded.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

#[test]
fn rejects_local_float_const_overflow_in_typed_default_and_any_slots() {
    let cases = [
        "import \"fmt\"\nfn main() {\n    const big = 1e300 * 1e300\n    var x f64 = big\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    const big = 1e40\n    var x f32 = big\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    const big = 1e300 * 1e300\n    x := big\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    const big = 1e300 * 1e300\n    fmt.println(big)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
            "expected HYB-E6002 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn rejects_folded_float_constants_assigned_to_integer_types() {
    let cases = [
        "import \"fmt\"\nconst f_val = 2.5\nfn main() {\n    var x int = f_val\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nconst f_val = 2.5\nfn main() {\n    var x int = 0\n    x = f_val\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    var x int = 1.25 + 1.25\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nconst big = 300.0\nfn main() {\n    var x int8 = big\n    fmt.println(x)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
            "expected HYB-E6002 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn rejects_folded_const_expressions_in_conversion_calls() {
    let cases = [
        "import \"fmt\"\nfn main() {\n    x := int64(1 << 70)\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nconst k = 1 << 70\nfn main() {\n    x := int64(k)\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    x := f32(1e38 * 10.0)\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    x := int8(1.25 + 1.25)\n    fmt.println(x)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E6006"),
            "expected HYB-E6006 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn accepts_in_range_float_consts_and_shadowed_conversion_callees() {
    let cases = [
        "import \"fmt\"\nconst whole = 2.0\nfn main() {\n    var x f32 = whole\n    var y int = int(whole)\n    fmt.println(x, y)\n}\n",
        "import \"fmt\"\nfn int8(n int) int {\n    return n\n}\nfn main() {\n    fmt.println(int8(1000))\n}\n",
        "import \"fmt\"\nconst big = 1e40\nfn f(big f32) {\n    var x f32 = big\n    fmt.println(x)\n}\nfn main() {\n    f(1.5)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}
