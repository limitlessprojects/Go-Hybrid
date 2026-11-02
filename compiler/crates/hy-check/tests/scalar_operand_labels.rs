//! scalar_operand_labels.rs — HYB-E1002 messages for a numeric WIDTH mismatch
//! name the exact Go types. Before this, `OpKind::label()` collapsed every
//! integer to `int` and every float to `float64`, so `i32 == i64` reported
//! "found `int` and `int`" — a message that contradicts itself.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn messages(src: &str) -> Vec<String> {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let r = check_file(&ast);
    assert_eq!(r.verdict, Verdict::Reject, "expected a rejection for:\n{src}");
    r.diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002")
        .map(|d| d.message.clone())
        .collect()
}

fn assert_named(src: &str, a: &str, b: &str) {
    let ms = messages(src);
    assert!(
        ms.iter().any(|m| m.contains(&format!("`{a}`")) && m.contains(&format!("`{b}`"))),
        "no E1002 message names `{a}` and `{b}`: {ms:?}"
    );
}

#[test]
fn equality_names_both_widths() {
    assert_named(
        "fn main() {\n    var a i32 = 3\n    var b i64 = 3\n    _ = a == b\n}\n",
        "int32",
        "int64",
    );
}

#[test]
fn addition_names_both_widths() {
    assert_named(
        "fn main() {\n    var a i32 = 3\n    var b i64 = 3\n    _ = a + b\n}\n",
        "int32",
        "int64",
    );
}

#[test]
fn ordered_and_arith_name_float_widths() {
    let src = "fn main() {\n    var a f32 = 1.5\n    var b f64 = 2.5\n    _ = a < b\n    _ = a - b\n}\n";
    let ms = messages(src);
    assert_eq!(ms.len(), 2, "{ms:?}");
    for m in &ms {
        assert!(m.contains("`float32`") && m.contains("`float64`"), "{m}");
    }
}

#[test]
fn bitwise_names_signedness() {
    assert_named(
        "fn main() {\n    var c u8 = 1\n    var d i8 = 2\n    _ = c | d\n}\n",
        "uint8",
        "int8",
    );
}

#[test]
fn compound_assignments_name_both_widths() {
    let src = "fn main() {\n    var a i32 = 3\n    var b i64 = 3\n    a += b\n    a -= b\n    a %= b\n    _ = a\n}\n";
    let ms = messages(src);
    assert_eq!(ms.len(), 3, "{ms:?}");
    for m in &ms {
        assert!(m.contains("`int32`") && m.contains("`int64`"), "{m}");
    }
}

#[test]
fn plain_assignment_names_both_widths() {
    assert_named(
        "fn main() {\n    var a f32 = 1.5\n    var b f64 = 2.5\n    a = b\n    _ = a\n}\n",
        "float64",
        "float32",
    );
}

#[test]
fn kind_mismatch_keeps_kind_labels() {
    // Untyped/unknown widths still fall back to the coarse kind label.
    let ms = messages("fn main() {\n    s := \"x\"\n    _ = s - 1\n}\n");
    assert!(ms.iter().any(|m| m.contains("`string`")), "{ms:?}");
}
