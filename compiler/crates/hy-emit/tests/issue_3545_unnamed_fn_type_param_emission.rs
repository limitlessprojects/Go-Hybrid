//! issue_3545_unnamed_fn_type_param_emission.rs — regression tests for issue #3545 (Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check` must emit Go code
//! that successfully compiles under `go build`.
//!
//! A function type with unnamed parameters whose type is a Hybrid width spelling
//! (`fn(i64) i64`, `fn(u8, i32) int`, `fn(f64)`) passed `hyb check` with 0 diagnostics,
//! but the emitter wrote the width spelling verbatim (`func(i64) int64`), so `go build`
//! failed with `undefined: i64`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    emit_go(&ast).expect("snippet emits")
}

#[test]
fn unnamed_fn_type_params_lower_hybrid_width_spellings() {
    // f1: local var declaration
    let src1 = "\
package main

fn main() {
    var g fn(i64) i64 = fn(x i64) i64 { return x + 1 }
    _ = g
}
";
    let go1 = emit(src1);
    assert!(
        go1.contains("var g func(int64) int64 = func(x int64)"),
        "expected 'var g func(int64) int64 = func(x int64)', got:\n{go1}"
    );
    assert!(!go1.contains("func(i64)"), "must not emit func(i64), got:\n{go1}");

    // f2: parameter position
    let src2 = "\
package main

fn ap(g fn(i64) i64, x i64) i64 {
    return g(x)
}

fn main() {}
";
    let go2 = emit(src2);
    assert!(
        go2.contains("func ap(g func(int64) int64, x int64) int64"),
        "expected 'func ap(g func(int64) int64, x int64) int64', got:\n{go2}"
    );

    // f3: type alias
    let src3 = "\
package main

type f = fn(i64) i64

fn main() {
    var g f = fn(x i64) i64 { return x + 1 }
    _ = g
}
";
    let go3 = emit(src3);
    assert!(
        go3.contains("type f = func(int64) int64"),
        "expected 'type f = func(int64) int64', got:\n{go3}"
    );

    // f4: multiple unnamed width parameters
    let src4 = "\
package main

type f = fn(u8, i32) int

fn main() {}
";
    let go4 = emit(src4);
    assert!(
        go4.contains("type f = func(uint8, int32) int"),
        "expected 'type f = func(uint8, int32) int', got:\n{go4}"
    );

    // f5: float width spellings
    let src5 = "\
package main

type f = fn(f64) f32

fn main() {}
";
    let go5 = emit(src5);
    assert!(
        go5.contains("type f = func(float64) float32"),
        "expected 'type f = func(float64) float32', got:\n{go5}"
    );

    // f6: named parameters control
    let src6 = "\
package main

type f = fn(x i64) i64

fn main() {}
";
    let go6 = emit(src6);
    assert!(
        go6.contains("type f = func(x int64) int64"),
        "expected 'type f = func(x int64) int64', got:\n{go6}"
    );

    // f7: mixed named and unnamed parameters (Go requires _ name placeholder)
    let src7 = "\
package main

type f = fn(x i64, u8) int

fn main() {}
";
    let go7 = emit(src7);
    assert!(
        go7.contains("type f = func(x int64, _ uint8) int"),
        "expected 'type f = func(x int64, _ uint8) int', got:\n{go7}"
    );
}
