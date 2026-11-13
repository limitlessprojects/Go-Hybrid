//! struct_literal_numeric_mismatch_2782.rs — issue #2782 (Row 13 BUILDABLE).
//!
//! `check_struct_literal` compared only `OpKind`, and every integer width
//! maps to `Int` while both float widths map to `Float`, so `point{x: i}`
//! with `i int` into an `int64` field (and `float64` into `float32`) passed
//! `hyb check` and failed `go build` (`cannot use i (variable of type int) as
//! int64 value in struct literal`). Untyped constants stay assignable.

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
fn mismatched_concrete_numeric_fields_reject() {
    // The issue's own minimal reproducer (snake_cased): `int` into `int64`,
    // `float64` into `float32`.
    let r = check(
        "struct point {\n    x int64\n    y float32\n}\nfn main() {\n    var i int = 42\n    var f float64 = 3.14\n    _ = point{\n        x: i,\n        y: f,\n    }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {codes:?}"
    );

    for (name, src) in [
        (
            "int var into float64 field",
            "struct p {\n    v float64\n}\nfn main() {\n    var i int = 42\n    _ = p{ v: i }\n}\n",
        ),
        (
            "float32 var into float64 field",
            "struct p {\n    v float64\n}\nfn main() {\n    var f float32 = 1.0\n    _ = p{ v: f }\n}\n",
        ),
        (
            "int64 var into int32 field",
            "struct p {\n    v int32\n}\nfn main() {\n    var n int64 = 42\n    _ = p{ v: n }\n}\n",
        ),
        (
            "uint64 var into uint32 field",
            "struct p {\n    v uint32\n}\nfn main() {\n    var n uint64 = 42\n    _ = p{ v: n }\n}\n",
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
fn matching_types_and_untyped_constants_stay_accepted() {
    for (name, src) in [
        (
            "untyped constants adapt",
            "struct point {\n    x int64\n    y float32\n}\nfn main() {\n    _ = point{\n        x: 42,\n        y: 3.14,\n    }\n}\n",
        ),
        (
            "exact widths",
            "struct point {\n    x int64\n    y float32\n}\nfn main() {\n    var i int64 = 42\n    var f float32 = 3.14\n    _ = point{\n        x: i,\n        y: f,\n    }\n}\n",
        ),
        (
            "byte/uint8 and rune/int32 aliases",
            "struct p {\n    b uint8\n    r int32\n}\nfn main() {\n    var x byte = 1\n    var y rune = 2\n    _ = p{ b: x, r: y }\n}\n",
        ),
        (
            "non-numeric fields are unaffected",
            "struct p {\n    s string\n}\nfn main() {\n    var s string = \"ok\"\n    _ = p{ s: s }\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
