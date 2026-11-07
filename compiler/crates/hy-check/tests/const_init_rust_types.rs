//! const_init_rust_types.rs - regression tests for issue #4002.
//!
//! Rust-only primitive aliases (`i8`, `u8`, `i16`, `u16`, `i32`, `u32`,
//! `i64`, `u64`, `f32`, `f64`, `isize`, `usize`, `char`) in `is_conversion_type_name`
//! previously caused non-Go conversion calls (`const x = i8(42)`,
//! `const y = usize(10)`) to be accepted as valid constant expressions
//! under `check_const_init_is_constant`.
//!
//! Because `hy-emit` does not lower these type names in Go expressions,
//! downstream `go build` fails with `undefined: i8`, violating Row 13 (BUILDABLE).
//!
//! Verifies that non-Go type conversions in `const` initializers reject with `HYB-E6003`,
//! while standard Go types (`int8`, `uint8`, `float32`, `complex128`) accept cleanly.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).expect("test input must parse cleanly");
    check_file(path, &ast)
}

#[test]
fn const_init_rejects_rust_conversion_names() {
    let cases = [
        "package main\nconst x = i8(42)",
        "package main\nconst x = u8(42)",
        "package main\nconst x = i16(42)",
        "package main\nconst x = u16(42)",
        "package main\nconst x = i32(42)",
        "package main\nconst x = u32(42)",
        "package main\nconst x = i64(42)",
        "package main\nconst x = u64(42)",
        "package main\nconst x = f32(1.0)",
        "package main\nconst x = f64(1.0)",
        "package main\nconst x = isize(10)",
        "package main\nconst x = usize(10)",
        "package main\nconst x = char(65)",
    ];

    for src in cases {
        let res = check(src);
        assert!(
            res.diags.iter().any(|d| d.code == "HYB-E6003"),
            "expected HYB-E6003 for source:\n{}",
            src
        );
    }
}

#[test]
fn const_init_accepts_standard_go_conversion_names() {
    let src = r#"
package main

const a = int8(42)
const b = uint8(255)
const c = float32(3.14)
const d = float64(2.718)
const e = complex128(complex(1.0, 2.0))
"#;

    let res = check(src);
    let errors: Vec<_> = res
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "standard Go type conversions in const must accept with 0 errors: {:?}",
        errors
    );
}
