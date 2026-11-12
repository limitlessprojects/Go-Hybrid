use hy_check::check_file;
use hy_syntax::parse_file;

fn e1002(src: &str) -> bool {
    check_file(&parse_file("t.hy", src).unwrap())
        .diagnostics
        .iter()
        .any(|x| x.code == "HYB-E1002")
}

/// The emitter writes `f64` as `float64` (and every other SPEC primitive
/// spelling as its Go name), so a local named `f64` does not shadow the
/// emitted type: Go builds `f64 := 1; var g float64 = 2.0`.
#[test]
fn local_named_like_renamed_primitive_is_not_a_shadow() {
    for (spec, val) in [
        ("f64", "2.0"),
        ("f32", "2.0"),
        ("i64", "2"),
        ("i32", "2"),
        ("i16", "2"),
        ("i8", "2"),
        ("u64", "2"),
        ("u32", "2"),
        ("u16", "2"),
        ("u8", "2"),
    ] {
        let src = format!("fn main() {{ {spec} := 1\n var g {spec} = {val}\n _ = {spec}\n _ = g }}");
        assert!(!e1002(&src), "false positive: {src}");
    }
}

/// The reverse: a local named with the Go spelling DOES shadow the emitted
/// type, so `float64 := 1; var g f64 = 2.0` (emitted `var g float64`) fails
/// `go build` with `float64 (variable) is not a type`.
#[test]
fn local_named_like_go_spelling_shadows_renamed_primitive() {
    for (go, spec, val) in [
        ("float64", "f64", "2.0"),
        ("int64", "i64", "2"),
        ("uint8", "u8", "2"),
    ] {
        let src = format!("fn main() {{ {go} := 1\n var g {spec} = {val}\n _ = {go}\n _ = g }}");
        assert!(e1002(&src), "missed shadow: {src}");
    }
}

/// Unrenamed spellings keep the #3029 behaviour.
#[test]
fn same_spelling_types_still_rejected() {
    for src in [
        "fn main() { int := 1\n var y int = 2\n _ = int\n _ = y }",
        "fn main() { float64 := 1\n var y float64 = 2.0\n _ = float64\n _ = y }",
    ] {
        assert!(e1002(src), "{src}");
    }
}
