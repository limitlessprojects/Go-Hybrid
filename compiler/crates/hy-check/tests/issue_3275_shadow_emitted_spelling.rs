use hy_check::check_file;
use hy_syntax::parse_file;

fn has_e1002(src: &str) -> bool {
    let parsed = parse_file("test.hy", src).expect("parse failed");
    let result = check_file(&parsed);
    result.diagnostics.iter().any(|d| d.code == "HYB-E1002")
}

const PRIMITIVE_PAIRS: [(&str, &str, &str); 10] = [
    ("u8", "uint8", "1"),
    ("u16", "uint16", "1"),
    ("u32", "uint32", "1"),
    ("u64", "uint64", "1"),
    ("i8", "int8", "1"),
    ("i16", "int16", "1"),
    ("i32", "int32", "1"),
    ("i64", "int64", "1"),
    ("f32", "float32", "1.0"),
    ("f64", "float64", "1.0"),
];

/// 1. False positive check: A local variable named with the SPEC spelling
/// (e.g. `f64 := 1`) does NOT shadow the emitted type (`var g float64 = 1.0`).
/// In Go, the variable is `f64` while the type is `float64`, which compiles cleanly.
#[test]
fn test_spec_spelling_local_does_not_shadow_emitted_type() {
    for (spec, _go, val) in PRIMITIVE_PAIRS {
        let src = format!(
            "fn main() {{\n    {spec} := 1\n    var g {spec} = {val}\n    _ = {spec}\n    _ = g\n}}\n"
        );
        assert!(
            !has_e1002(&src),
            "false positive: local `{spec}` should not shadow emitted type for `{spec}`\nsrc:\n{src}"
        );
    }
}

/// 2. False negative check: A local variable named with the Go spelling
/// (e.g. `float64 := 1`) DOES shadow the emitted type (`var g float64 = 1.0`).
/// In Go, `float64 (variable) is not a type` fails `go build`.
#[test]
fn test_go_spelling_local_shadows_spec_spelling_type() {
    for (spec, go, val) in PRIMITIVE_PAIRS {
        let src = format!(
            "fn main() {{\n    {go} := 1\n    var g {spec} = {val}\n    _ = {go}\n    _ = g\n}}\n"
        );
        assert!(
            has_e1002(&src),
            "false negative: local `{go}` must shadow emitted type for `{spec}`\nsrc:\n{src}"
        );
    }
}

/// 3. Parameter named with SPEC spelling does NOT shadow emitted type.
#[test]
fn test_spec_spelling_param_does_not_shadow_emitted_type() {
    for (spec, _go, val) in PRIMITIVE_PAIRS {
        let src = format!(
            "fn f({spec} int) {{\n    var g {spec} = {val}\n    _ = {spec}\n    _ = g\n}}\nfn main() {{ f(1) }}\n"
        );
        assert!(
            !has_e1002(&src),
            "false positive: parameter `{spec}` should not shadow emitted type for `{spec}`\nsrc:\n{src}"
        );
    }
}

/// 4. Parameter named with Go spelling DOES shadow emitted type.
#[test]
fn test_go_spelling_param_shadows_spec_spelling_type() {
    for (spec, go, val) in PRIMITIVE_PAIRS {
        let src = format!(
            "fn f({go} int) {{\n    var g {spec} = {val}\n    _ = {go}\n    _ = g\n}}\nfn main() {{ f(1) }}\n"
        );
        assert!(
            has_e1002(&src),
            "false negative: parameter `{go}` must shadow emitted type for `{spec}`\nsrc:\n{src}"
        );
    }
}

/// 5. Closure parameters and locals.
#[test]
fn test_closure_params_and_locals() {
    // Closure param with spec spelling: accepted
    let src1 = "fn main() {\n    f := fn(f64 int) {\n        var g f64 = 2.0\n        _ = f64\n        _ = g\n    }\n    f(1)\n}\n";
    assert!(!has_e1002(src1), "closure param with spec spelling must be accepted");

    // Closure param with Go spelling: rejected HYB-E1002
    let src2 = "fn main() {\n    f := fn(float64 int) {\n        var g f64 = 2.0\n        _ = float64\n        _ = g\n    }\n    f(1)\n}\n";
    assert!(has_e1002(src2), "closure param with Go spelling must reject HYB-E1002");

    // Closure local with spec spelling: accepted
    let src3 = "fn main() {\n    f := fn() {\n        i64 := 1\n        var g i64 = 2\n        _ = i64\n        _ = g\n    }\n    f()\n}\n";
    assert!(!has_e1002(src3), "closure local with spec spelling must be accepted");

    // Closure local with Go spelling: rejected HYB-E1002
    let src4 = "fn main() {\n    f := fn() {\n        int64 := 1\n        var g i64 = 2\n        _ = int64\n        _ = g\n    }\n    f()\n}\n";
    assert!(has_e1002(src4), "closure local with Go spelling must reject HYB-E1002");
}

/// 6. Composite types: slices, arrays, maps, channels, pointers, options.
#[test]
fn test_composite_type_uses() {
    // Slice literal
    let src_slice_ok = "fn main() {\n    f64 := 1\n    xs := []f64{1.0}\n    _ = f64\n    _ = xs\n}\n";
    assert!(!has_e1002(src_slice_ok), "slice literal with spec spelling local must be accepted");

    let src_slice_bad = "fn main() {\n    float64 := 1\n    xs := []f64{1.0}\n    _ = float64\n    _ = xs\n}\n";
    assert!(has_e1002(src_slice_bad), "slice literal with Go spelling local must reject");

    // Sized array literal
    let src_arr_ok = "fn main() {\n    i64 := 1\n    a := [2]i64{1, 2}\n    _ = i64\n    _ = a\n}\n";
    assert!(!has_e1002(src_arr_ok), "array literal with spec spelling local must be accepted");

    let src_arr_bad = "fn main() {\n    int64 := 1\n    a := [2]i64{1, 2}\n    _ = int64\n    _ = a\n}\n";
    assert!(has_e1002(src_arr_bad), "array literal with Go spelling local must reject");

    // Map literal key & value
    let src_map_ok = "fn main() {\n    u8 := 1\n    i32 := 2\n    m := map[u8]i32{1: 2}\n    _ = u8\n    _ = i32\n    _ = m\n}\n";
    assert!(!has_e1002(src_map_ok), "map literal with spec spelling locals must be accepted");

    let src_map_key_bad = "fn main() {\n    uint8 := 1\n    m := map[u8]i32{1: 2}\n    _ = uint8\n    _ = m\n}\n";
    assert!(has_e1002(src_map_key_bad), "map literal with Go spelling key local must reject");

    let src_map_val_bad = "fn main() {\n    int32 := 1\n    m := map[u8]i32{1: 2}\n    _ = int32\n    _ = m\n}\n";
    assert!(has_e1002(src_map_val_bad), "map literal with Go spelling val local must reject");

    // Channel type
    let src_chan_ok = "fn main() {\n    f32 := 1\n    var c chan f32\n    _ = f32\n    _ = c\n}\n";
    assert!(!has_e1002(src_chan_ok), "channel with spec spelling local must be accepted");

    let src_chan_bad = "fn main() {\n    float32 := 1\n    var c chan f32\n    _ = float32\n    _ = c\n}\n";
    assert!(has_e1002(src_chan_bad), "channel with Go spelling local must reject");

    // Pointer type
    let src_ptr_ok = "fn main() {\n    u16 := 1\n    var p *u16\n    _ = u16\n    _ = p\n}\n";
    assert!(!has_e1002(src_ptr_ok), "pointer with spec spelling local must be accepted");

    let src_ptr_bad = "fn main() {\n    uint16 := 1\n    var p *u16\n    _ = uint16\n    _ = p\n}\n";
    assert!(has_e1002(src_ptr_bad), "pointer with Go spelling local must reject");

    // Option type
    let src_opt_ok = "fn main() {\n    u64 := 1\n    var o option[u64]\n    _ = u64\n    _ = o\n}\n";
    assert!(!has_e1002(src_opt_ok), "option with spec spelling local must be accepted");

    let src_opt_bad = "fn main() {\n    uint64 := 1\n    var o option[u64]\n    _ = uint64\n    _ = o\n}\n";
    assert!(has_e1002(src_opt_bad), "option with Go spelling local must reject");
}

/// 7. Scope lifecycle: block scopes.
#[test]
fn test_block_scope_lifecycle() {
    // Inner block shadow rejects
    let src_block_bad = "fn main() {\n    {\n        float64 := 1\n        var g f64 = 2.0\n        _ = float64\n        _ = g\n    }\n}\n";
    assert!(has_e1002(src_block_bad), "inner block shadow must reject HYB-E1002");

    // After inner block ends, outer scope use is valid
    let src_block_ok = "fn main() {\n    {\n        float64 := 1\n        _ = float64\n    }\n    var g f64 = 2.0\n    _ = g\n}\n";
    assert!(!has_e1002(src_block_ok), "type use after inner block ends must be accepted");
}

/// 8. Same-spelling and Go aliases:
/// - `int`, `string`, `bool`, `byte`, `rune` same-name shadows still reject
/// - `byte := 1; var b uint8 = 2` is accepted because in Go `byte` does not shadow `uint8`
/// - `rune := 1; var r int32 = 2` is accepted because in Go `rune` does not shadow `int32`
#[test]
fn test_unrenamed_and_alias_preservation() {
    for src in [
        "fn main() { int := 1\n var y int = 2\n _ = int\n _ = y }",
        "fn main() { string := `a`\n var s string = `b`\n _ = string\n _ = s }",
        "fn main() { bool := true\n var b bool = false\n _ = bool\n _ = b }",
        "fn main() { byte := 1\n var b byte = 2\n _ = byte\n _ = b }",
        "fn main() { rune := 'a'\n var r rune = 'b'\n _ = rune\n _ = r }",
    ] {
        assert!(has_e1002(src), "unrenamed same-spelling shadow must reject: {src}");
    }

    assert!(!has_e1002("fn main() {\n    byte := 1\n    var b uint8 = 2\n    _ = byte\n    _ = b\n}\n"));
    assert!(!has_e1002("fn main() {\n    rune := 'a'\n    var r int32 = 2\n    _ = rune\n    _ = r\n}\n"));
}
