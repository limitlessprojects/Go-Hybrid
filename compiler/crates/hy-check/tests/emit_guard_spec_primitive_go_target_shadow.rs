//! Regression tests for `emit_guard.rs` rejecting SPEC primitive type spellings
//! (`u8`, `i64`, `f64`, ...) and conversion calls (`u8(x)`, `f64(x)`, ...) when
//! their lowered Go target primitive name (`uint8`, `int64`, `float64`, ...) is
//! shadowed by an in-scope local variable or parameter (`HYB-E1002`, GUARANTEES
//! Row 13 `BUILDABLE`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parses");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn rejects_spec_primitive_types_when_go_target_name_is_shadowed_in_scope() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    uint8 := 5
    var x u8 = 1
    fmt.println(uint8, x)
}
"#,
        ),
        vec!["HYB-E1002"],
        "`var x u8` must reject with HYB-E1002 when `uint8` is shadowed by a local"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    float64 := 5
    xs := []f64{1.5}
    fmt.println(float64, len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "`[]f64` must reject with HYB-E1002 when `float64` is shadowed by a local"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn run(int64 int) {
    var x i64 = 10
    fmt.println(int64, x)
}
fn main() {
    run(1)
}
"#,
        ),
        vec!["HYB-E1002"],
        "`var x i64` must reject with HYB-E1002 when `int64` is shadowed by a parameter"
    );
}

#[test]
fn rejects_spec_primitive_conversion_calls_when_go_target_name_is_shadowed() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    uint8 := 5
    y := u8(uint8)
    fmt.println(y)
}
"#,
        ),
        vec!["HYB-E1002"],
        "`u8(...)` conversion must reject with HYB-E1002 when `uint8` is shadowed by a local"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    float64 := 5
    y := f64(float64)
    fmt.println(y)
}
"#,
        ),
        vec!["HYB-E1002"],
        "`f64(...)` conversion must reject with HYB-E1002 when `float64` is shadowed by a local"
    );

    assert!(
        codes(
            r#"import "fmt"
fn main() {
    float64 := fn(x int) float64 { return 2.0 }
    _ = 1.0 / float64(0)
    fmt.println(1)
}
"#,
        )
        .is_empty(),
        "calling a local closure named `float64` directly must stay accepted"
    );
}
