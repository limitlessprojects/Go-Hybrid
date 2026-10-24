//! go_type_shadowing_issue_728.rs — regression tests for issue #728:
//!
//! "A Hybrid identifier spelled like a Go type (`float64`, `int64`, `uint8`, ...)
//! captures the emitter's lowering of `f64(x)` / `i64(x)` / `u8(x)`: accepted
//! program builds and prints a wrong result (or fails `go build`)"
//!
//! The compiler must reject fail-closed with `HYB-E1002` (rule `BUILDABLE`)
//! whenever a package-level declaration, local variable, parameter, result,
//! loop variable, select recv, match arm binding, or type parameter collides with:
//!   - Lowered Go primitive types (`uint8`..`uint64`, `int8`..`int64`, `float32`, `float64`, `uintptr`, `complex64`, `complex128`)
//!   - Compiler runtime helpers (`hy_pop`)
//!   - Compiler-injected packages (`os`, `filepath`, `fmt`) when main has E6 error handling
//!   - Imported package names
//!
//! Legitimate uses of these names as types and conversions must continue to pass.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_728.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}\ncode:\n{}", diags, src));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?} on:\n{}",
        what,
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for: {}", what);
}

fn assert_accepts(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "expected accept for {}, got {:?} on:\n{}",
        what,
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected accept for: {}", what);
}

// ===========================================================================
// 1. Package-level declarations colliding with Go primitive types
// ===========================================================================

#[test]
fn top_level_fn_named_float64_rejects_e1002() {
    assert_rejects_e1002(
        "fn float64(x int) int { return x }\nfn main() { _ = float64(1) }",
        "top-level fn float64",
    );
}

#[test]
fn top_level_fn_named_int64_rejects_e1002() {
    assert_rejects_e1002(
        "fn int64(x int) int { return x }\nfn main() { _ = int64(1) }",
        "top-level fn int64",
    );
}

#[test]
fn top_level_const_named_int64_rejects_e1002() {
    assert_rejects_e1002(
        "const int64 = 42\nfn main() { _ = int64 }",
        "top-level const int64",
    );
}

#[test]
fn top_level_struct_named_uint8_rejects_e1002() {
    assert_rejects_e1002(
        "struct uint8 { val int }\nfn main() {\n    var s uint8\n    _ = s.val\n}",
        "top-level struct uint8",
    );
}

#[test]
fn top_level_enum_named_int32_rejects_e1002() {
    assert_rejects_e1002(
        "enum int32 { a, b }\nfn main() {\n    var x int32 = int32.a\n    _ = x\n}",
        "top-level enum int32",
    );
}

#[test]
fn top_level_type_alias_named_uint64_rejects_e1002() {
    assert_rejects_e1002(
        "type uint64 = int\nfn main() {\n    var x uint64 = 1\n    _ = x\n}",
        "top-level type alias uint64",
    );
}

// ===========================================================================
// 2. Compiler runtime helper (hy_pop) collision
// ===========================================================================

#[test]
fn top_level_fn_hy_pop_rejects_e1002() {
    assert_rejects_e1002(
        "fn hy_pop() int { return 1 }\nfn main() { _ = hy_pop() }",
        "top-level fn hy_pop",
    );
}

#[test]
fn local_hy_pop_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { hy_pop := 1\n_ = hy_pop }",
        "local hy_pop :=",
    );
}

// ===========================================================================
// 3. Injected packages when main has E6 error handling
// ===========================================================================

#[test]
fn top_level_fn_filepath_with_main_error_rejects_e1002() {
    assert_rejects_e1002(
        "fn filepath() int { return 1 }\nfn main() error {\n    _ = filepath()\n    return nil\n}",
        "top-level fn filepath with main error",
    );
}

#[test]
fn top_level_fn_os_with_main_error_rejects_e1002() {
    assert_rejects_e1002(
        "fn os() int { return 1 }\nfn main() error {\n    _ = os()\n    return nil\n}",
        "top-level fn os with main error",
    );
}

#[test]
fn top_level_fn_filepath_with_main_try_rejects_e1002() {
    assert_rejects_e1002(
        "fn get() (int, error) { return 1, nil }\nfn filepath() int { return 1 }\nfn main() { x := get()?\n_ = x\n_ = filepath() }",
        "top-level fn filepath with main try",
    );
}

// ===========================================================================
// 4. Imported package collision with package-level decl
// ===========================================================================

#[test]
fn top_level_fn_colliding_with_import_rejects_e1002() {
    assert_rejects_e1002(
        "import \"strings\"\nfn strings() int { return 1 }\nfn main() { _ = strings() }",
        "top-level fn colliding with import strings",
    );
}

#[test]
fn top_level_fn_colliding_with_import_after_it_rejects_e1002() {
    assert_rejects_e1002(
        "fn strings() int { return 1 }\nimport \"strings\"\nfn main() { _ = strings() }",
        "top-level fn colliding with subsequent import strings",
    );
}

// ===========================================================================
// 5. Local variable bindings colliding with Go primitive types
// ===========================================================================

#[test]
fn local_short_define_float64_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { float64 := 1.0\n_ = float64 }",
        "local float64 :=",
    );
}

#[test]
fn local_let_int64_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { let int64 = 42\n_ = int64 }",
        "local let int64",
    );
}

#[test]
fn local_var_uint8_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { var uint8 int = 5\n_ = uint8 }",
        "local var uint8",
    );
}

#[test]
fn local_const_uintptr_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { const uintptr = 10\n_ = uintptr }",
        "local const uintptr",
    );
}

// ===========================================================================
// 6. Function parameters, receivers, and named results
// ===========================================================================

#[test]
fn fn_param_named_float64_rejects_e1002() {
    assert_rejects_e1002(
        "fn f(float64 int) int { return float64 }\nfn main() { _ = f(1) }",
        "param named float64",
    );
}

#[test]
fn fn_named_result_named_int64_rejects_e1002() {
    assert_rejects_e1002(
        "fn f() (int64 int) { int64 = 1\nreturn int64 }\nfn main() { _ = f() }",
        "named result int64",
    );
}

#[test]
fn method_receiver_named_uint8_rejects_e1002() {
    assert_rejects_e1002(
        "struct my_s { v int }\nfn (uint8 my_s) get() int { return uint8.v }\nfn main() {\n    var s my_s\n    _ = s.get()\n}",
        "method receiver uint8",
    );
}

// ===========================================================================
// 7. Closures (FnLit)
// ===========================================================================

#[test]
fn closure_param_named_uint8_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { f := fn(uint8 int) int { return uint8 }\n_ = f(1) }",
        "closure param uint8",
    );
}

#[test]
fn closure_named_result_named_float32_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { f := fn() (float32 int) { float32 = 1\nreturn float32 }\n_ = f() }",
        "closure named result float32",
    );
}

// ===========================================================================
// 8. Loops, Select, Match, and Type Parameters
// ===========================================================================

#[test]
fn for_range_var_named_float64_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { for float64 in 0..10 { _ = float64 } }",
        "for range var float64",
    );
}

#[test]
fn for_iterate_var_named_int64_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() { xs := []int{1, 2}\nfor int64 in xs { _ = int64 } }",
        "for iterate var int64",
    );
}

#[test]
fn select_recv_named_uint8_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n    uint8 := <-ch:\n        _ = uint8\n    default:\n    }\n}",
        "select recv named uint8",
    );
}

#[test]
fn match_binding_named_int16_rejects_e1002() {
    assert_rejects_e1002(
        "fn main() {\n    opt := some(1)\n    match opt {\n        some(int16) => {\n            _ = int16\n        }\n        none => {}\n    }\n}",
        "match binding int16",
    );
}

#[test]
fn generic_type_param_named_float64_rejects_e1002() {
    assert_rejects_e1002(
        "fn id[float64 any](x float64) float64 { return x }\nfn main() { _ = id(1) }",
        "type param named float64",
    );
}

// ===========================================================================
// 9. Positive Controls: Legitimate type expressions and conversions MUST pass
// ===========================================================================

#[test]
fn legitimate_primitive_types_and_conversions_accept() {
    assert_accepts(
        "fn main() {\n    var a uint8 = 1\n    var b uint16 = 2\n    var c uint32 = 3\n    var d uint64 = 4\n    var e int8 = 5\n    var f int16 = 6\n    var g int32 = 7\n    var h int64 = 8\n    var i float32 = 9.0\n    var j float64 = 10.0\n    _ = a\n    _ = b\n    _ = c\n    _ = d\n    _ = e\n    _ = f\n    _ = g\n    _ = h\n    _ = i\n    _ = j\n}",
        "all Go primitive type variables",
    );
}

#[test]
fn legitimate_type_alias_of_primitive_accepts() {
    assert_accepts(
        "type my_byte = uint8\ntype my_float = float64\nfn main() {\n    var x my_byte = 1\n    var y my_float = 2.5\n    _ = x\n    _ = y\n}",
        "type aliases of primitive types",
    );
}

#[test]
fn legitimate_conversions_and_lowered_types_accept() {
    assert_accepts(
        "fn main() {\n    x := uint8(200)\n    y := float64(42)\n    z := f64(7) / 2.0\n    w := u8(255)\n    _ = x\n    _ = y\n    _ = z\n    _ = w\n}",
        "conversions using uint8, float64, f64, u8",
    );
}

#[test]
fn legitimate_slice_pop_accepts() {
    assert_accepts(
        "fn main() {\n    var s = []int{1, 2, 3}\n    x := s.pop()\n    _ = x\n}",
        "slice .pop() method",
    );
}
