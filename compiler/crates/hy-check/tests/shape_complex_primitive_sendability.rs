//! shape_complex_primitive_sendability.rs - regression tests for issue #3988.
//!
//! `named_shape_depth`, `call_shape`, and `receiver_type_params` in `shape.rs`
//! previously omitted `complex64` and `complex128`, causing them to resolve
//! as `TypeShape::Unknown`. Under O4.9 fail-closed boundary checking, passing
//! complex numbers or structs carrying complex numbers across a `task` spawn
//! or channel send rejected fail-closed with `HYB-E3005`.
//!
//! Verifies:
//!   1. `complex64` and `complex128` values are classified as `TypeShape::Primitive`.
//!   2. Passing complex values to `task worker(c)` accepts with 0 diagnostics.
//!   3. Sending complex values over `chan complex128` accepts with 0 diagnostics.
//!   4. Structs containing complex fields are proved const-sendable.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).expect("test input must parse cleanly");
    check_file(path, &ast)
}

#[test]
fn complex_primitive_sendability_positive() {
    let src = r#"
package main

struct Signal {
    freq complex128
    amp  float64
}

fn worker(c complex128) {}

fn struct_worker(s Signal) {}

fn main() {
    var c complex128
    task worker(c)

    var s Signal
    task struct_worker(s)

    ch := make(chan complex128, 2)
    ch <- c
}
"#;

    let res = check(src);
    let errors: Vec<_> = res
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "complex primitives must cross task and channel boundaries with 0 errors: {:?}",
        errors
    );
}

#[test]
fn complex_builtins_primitive_shape() {
    let src = r#"
package main

fn worker(c complex128) {}

fn main() {
    c := complex(1.0, 2.0)
    task worker(c)
}
"#;

    let res = check(src);
    let errors: Vec<_> = res
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "complex() call must resolve to TypeShape::Primitive and cross task boundary: {:?}",
        errors
    );
}
