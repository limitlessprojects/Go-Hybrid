//! typed_int_float_coercion_issue_1112.rs — regression tests for issue #1112:
//! a typed `int` VALUE must not satisfy a `float64` target anywhere, while
//! Go's untyped-constant conversion (`var f float64 = 1`, `return 1`,
//! `const k = 3` …) must keep building at every one of those sites.
//!
//! Root cause: `kinds_conflict`'s `(Float, Int) => false` arm exists solely
//! for untyped int constants, but the kind model cannot tell an untyped
//! constant from an `int` variable — so `var y int = 3; var f float64 = y`
//! (which go build refuses) passed every value position. The fix routes
//! value positions through `Collector::value_conflicts`, which applies the
//! narrow clause only when the value expression is NOT an untyped constant
//! (`is_untyped_constant`: int/rune literals, `-x`, `a op b` over constants,
//! and references to untyped `const`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_int_float_coercion.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject_e1002(src: &str, label: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{label}: must reject");
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "{label}: expected HYB-E1002, got {:?}",
        error_codes(&r)
    );
}

fn assert_accept_clean(src: &str, label: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "{label}: must pass cleanly, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept, "{label}: must accept");
}

// ---------------------------------------------------------------------------
// Reject: the issue's value positions, one per surface
// ---------------------------------------------------------------------------

#[test]
fn assign_typed_int_to_float64_target_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var x float64
    x = y
    fmt.println(\"{x}\")
}
",
        "plain assignment",
    );
}

#[test]
fn init_typed_int_in_float64_declaration_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var x float64 = y
    fmt.println(\"{x}\")
}
",
        "typed declaration initializer",
    );
}

#[test]
fn array_literal_typed_int_element_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var a [2]float64 = [2]float64{y}
    fmt.println(\"{a}\")
}
",
        "array literal element",
    );
}

#[test]
fn slice_literal_typed_int_element_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var s []float64 = []float64{y}
    fmt.println(\"{s}\")
}
",
        "slice literal element",
    );
}

#[test]
fn append_typed_int_to_float64_slice_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var s []float64
    s = append(s, y)
    fmt.println(\"{s}\")
}
",
        "append element",
    );
}

#[test]
fn map_literal_value_typed_int_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    m := map[string]float64{\"a\": y}
    fmt.println(\"{m}\")
}
",
        "map literal value",
    );
}

#[test]
fn map_literal_key_typed_int_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    m := map[float64]string{y: \"a\"}
    fmt.println(\"{m}\")
}
",
        "map literal key",
    );
}

#[test]
fn map_index_assign_typed_int_value_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    m := map[string]float64{\"a\": 1.5}
    m[\"k\"] = y
    fmt.println(\"{m}\")
}
",
        "map index assignment value",
    );
}

#[test]
fn map_index_key_typed_int_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    m := map[float64]int{1.5: 2}
    q := m[y]
    fmt.println(\"{q}\")
}
",
        "map index key",
    );
}

#[test]
fn fn_arg_typed_int_to_float64_param_rejected() {
    assert_reject_e1002(
        "
fn g(x f64) f64 {
    return x
}

fn main() {
    var y int = 3
    fmt.println(\"{g(y)}\")
}
",
        "function argument (f64 alias)",
    );
}

#[test]
fn method_arg_typed_int_to_float64_param_rejected() {
    assert_reject_e1002(
        "
struct point {
    f float64
}

fn (p point) scale(v float64) float64 {
    return v
}

fn main() {
    var y int = 3
    var pt point
    r := pt.scale(y)
    fmt.println(\"{r}\")
}
",
        "method argument",
    );
}

#[test]
fn pointer_arg_addressable_typed_int_rejected() {
    assert_reject_e1002(
        "
fn take(p *float64) float64 {
    return p
}

fn main() {
    var y int = 3
    fmt.println(\"{take(y)}\")
}
",
        "pointer parameter fed an addressable int (auto-& path)",
    );
}

#[test]
fn return_typed_int_from_float64_fn_rejected() {
    assert_reject_e1002(
        "
fn h() f64 {
    var y int = 3
    return y
}

fn main() {
    fmt.println(\"{h()}\")
}
",
        "return statement",
    );
}

#[test]
fn return_int_call_from_float64_fn_rejected() {
    assert_reject_e1002(
        "
fn g() int {
    return 1
}

fn h() float64 {
    return g()
}

fn main() {
    fmt.println(\"{h()}\")
}
",
        "return of an int-returning call (multi-result zip path)",
    );
}

#[test]
fn send_typed_int_on_chan_float64_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var ch chan float64
    ch <- y
    fmt.println(\"sent\")
}
",
        "channel send via var chan",
    );
}

#[test]
fn send_typed_int_on_make_chan_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    ch := make(chan float64, 1)
    ch <- y
    fmt.println(\"sent\")
}
",
        "channel send via make(chan float64)",
    );
}

#[test]
fn struct_field_assign_typed_int_rejected() {
    assert_reject_e1002(
        "
struct point {
    f float64
}

fn main() {
    var p point
    var y int = 3
    p.f = y
    fmt.println(\"{p.f}\")
}
",
        "struct field assignment",
    );
}

#[test]
fn struct_literal_field_typed_int_rejected() {
    assert_reject_e1002(
        "
struct point {
    f float64
}

fn main() {
    var y int = 3
    p := point{f: y}
    fmt.println(\"{p.f}\")
}
",
        "struct literal field",
    );
}

#[test]
fn compound_add_assign_typed_int_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var x float64 = 1.5
    var y int = 3
    x += y
    fmt.println(\"{x}\")
}
",
        "+= with typed int",
    );
}

#[test]
fn compound_arith_assign_typed_int_rejected() {
    for op in ["-=", "*=", "/="] {
        let src = format!(
            "
fn main() {{
    var x float64 = 1.5
    var y int = 3
    x {op} y
    fmt.println(\"{{x}}\")
}}
"
        );
        assert_reject_e1002(&src, &format!("`{op}` with typed int"));
    }
}

#[test]
fn parallel_assign_typed_int_to_float64_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var pf float64 = 1.0
    var xi int = 0
    pf, xi = y, y
    fmt.println(\"{pf} {xi}\")
}
",
        "parallel assignment positional pair",
    );
}

#[test]
fn copy_mismatched_element_types_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var fs []float64
    var is []int
    n := copy(fs, is)
    fmt.println(\"{n}\")
}
",
        "copy([]float64, []int)",
    );
}

#[test]
fn typed_file_const_int_to_float64_rejected() {
    assert_reject_e1002(
        "
const k int = 3

fn main() {
    var f float64 = k
    fmt.println(\"{f}\")
}
",
        "file-level TYPED const `const k int = 3`",
    );
}

#[test]
fn binary_var_expression_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var f float64 = y + 1
    fmt.println(\"{f}\")
}
",
        "binary expression over a typed variable",
    );
}

#[test]
fn unary_neg_var_expression_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var f float64 = -y
    fmt.println(\"{f}\")
}
",
        "unary minus over a typed variable",
    );
}

#[test]
fn f32_target_typed_int_rejected() {
    assert_reject_e1002(
        "
fn main() {
    var y int = 3
    var f float32 = y
    fmt.println(\"{f}\")
}
",
        "float32 target",
    );
}

// ---------------------------------------------------------------------------
// Accept: every Go-legal shape at the SAME sites must stay clean
// ---------------------------------------------------------------------------

#[test]
fn untyped_constant_controls_pass_cleanly() {
    let controls = [
        // literal init / neg / binary constant expression
        ("literal init", "fn main() {\n    var f float64 = 1\n    fmt.println(\"{f}\")\n}\n"),
        ("neg literal init", "fn main() {\n    var f float64 = -1\n    fmt.println(\"{f}\")\n}\n"),
        ("binary constant expr", "fn main() {\n    var f float64 = 1 + 2\n    fmt.println(\"{f}\")\n}\n"),
        // rune literal (untyped constant, converts in Go)
        ("rune literal", "fn main() {\n    var f float64 = 'x'\n    fmt.println(\"{f}\")\n}\n"),
        // assignment of a constant
        ("assign literal", "fn main() {\n    var f float64 = 1.5\n    f = 2\n    fmt.println(\"{f}\")\n}\n"),
        // array / slice / map literals of constants
        ("array lit literals", "fn main() {\n    var a [2]float64 = [2]float64{1, 2}\n    fmt.println(\"{a}\")\n}\n"),
        ("slice lit literals", "fn main() {\n    var s []float64 = []float64{1, 2}\n    fmt.println(\"{s}\")\n}\n"),
        ("map lit literals", "fn main() {\n    m := map[string]float64{\"a\": 1}\n    fmt.println(\"{m}\")\n}\n"),
        ("map lit float key literal", "fn main() {\n    m := map[float64]string{1: \"a\"}\n    fmt.println(\"{m}\")\n}\n"),
        // append of a constant
        ("append literal", "fn main() {\n    var s []float64\n    s = append(s, 1)\n    fmt.println(\"{s}\")\n}\n"),
        // argument of a constant
        ("arg literal", "fn g(x float64) float64 {\n    return x\n}\nfn main() {\n    fmt.println(\"{g(2)}\")\n}\n"),
        // return of a constant — the classic face
        ("return literal", "fn f() float64 {\n    return 1\n}\nfn main() {\n    fmt.println(\"{f()}\")\n}\n"),
        // compound assignment with a constant
        ("+= literal", "fn main() {\n    var f float64 = 1.5\n    f += 1\n    fmt.println(\"{f}\")\n}\n"),
        // same-type float flow must stay untouched
        ("float to float", "fn main() {\n    var a float64 = 1.5\n    var b float64 = a\n    b = a\n    fmt.println(\"{b}\")\n}\n"),
        // explicit conversion is legal Go — never reject
        ("explicit conversion", "fn main() {\n    var y int = 3\n    var f float64 = float64(y)\n    fmt.println(\"{f}\")\n}\n"),
        // f32 alias target with a constant
        ("f32 literal", "fn main() {\n    var f f32 = 1\n    fmt.println(\"{f}\")\n}\n"),
    ];
    for (label, src) in controls {
        assert_accept_clean(src, label);
    }
}

#[test]
fn untyped_file_const_controls_pass_cleanly() {
    let controls = [
        ("init", "const k = 3\n\nfn main() {\n    var f float64 = k\n    fmt.println(\"{f}\")\n}\n"),
        ("assign", "const k = 3\n\nfn main() {\n    var f float64 = 1.0\n    f = k\n    fmt.println(\"{f}\")\n}\n"),
        ("return", "const k = 3\n\nfn f() float64 {\n    return k\n}\nfn main() {\n    fmt.println(\"{f()}\")\n}\n"),
        ("arg", "const k = 3\n\nfn g(x float64) float64 {\n    return x\n}\nfn main() {\n    fmt.println(\"{g(k)}\")\n}\n"),
        ("append", "const k = 3\n\nfn main() {\n    var s []float64\n    s = append(s, k)\n    fmt.println(\"{s}\")\n}\n"),
        ("array lit", "const k = 3\n\nfn main() {\n    var a [2]float64 = [2]float64{k}\n    fmt.println(\"{a}\")\n}\n"),
        ("slice lit", "const k = 3\n\nfn main() {\n    var s []float64 = []float64{k}\n    fmt.println(\"{s}\")\n}\n"),
        ("map value", "const k = 3\n\nfn main() {\n    m := map[string]float64{\"a\": k}\n    fmt.println(\"{m}\")\n}\n"),
        ("send", "const k = 3\n\nfn main() {\n    var ch chan float64\n    ch <- k\n    fmt.println(\"sent\")\n}\n"),
        ("local const", "fn main() {\n    const k = 3\n    var f float64 = k\n    fmt.println(\"{f}\")\n}\n"),
        ("compound +=", "const k = 3\n\nfn main() {\n    var f float64 = 1.5\n    f += k\n    fmt.println(\"{f}\")\n}\n"),
    ];
    for (label, src) in controls {
        assert_accept_clean(src, label);
    }
}

#[test]
fn equality_and_conversion_controls_pass_cleanly() {
    let controls = [
        // untyped constant comparison must stay legal (`1 == 1.0` builds)
        ("int-float const equality", "fn main() {\n    var y int = 1\n    if y == 1 {\n        fmt.println(\"eq\")\n    }\n}\n"),
        ("float const comparison", "fn main() {\n    if 1 == 1.0 {\n        fmt.println(\"eq\")\n    }\n}\n"),
        // explicit int→float conversion in every value position
        ("converted arg", "fn g(x float64) float64 {\n    return x\n}\nfn main() {\n    var y int = 3\n    fmt.println(\"{g(float64(y))}\")\n}\n"),
        ("converted return", "fn h() float64 {\n    var y int = 3\n    return float64(y)\n}\nfn main() {\n    fmt.println(\"{h()}\")\n}\n"),
    ];
    for (label, src) in controls {
        assert_accept_clean(src, label);
    }
}
