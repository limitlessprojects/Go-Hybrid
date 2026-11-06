//! Regression tests for `HYB-E6002` constant overflow and float-to-int
//! truncation in fields of type-elided struct literals (`{...}`) inheriting
//! their head from slice, pointer-to-struct slice, fixed-size array, map
//! key/value, nested untyped slice, and generic struct composite slots
//! (`compiler/crates/hy-check/src/emit_guard.rs`, GUARANTEES Row 13 `BUILDABLE`).

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
fn rejects_const_overflow_in_elided_struct_fields_across_slice_ptr_and_array_slots() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct item { b u8 }
fn main() {
    xs := []item{{b: 300}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E6002"],
        "elided struct literal in []item with overflowing u8 field must reject HYB-E6002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct item { b u8 }
fn main() {
    xs := []*item{{b: 300}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E6002"],
        "elided struct literal in []*item with overflowing u8 field must reject HYB-E6002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct item { b i8 }
fn main() {
    a := [1]item{{b: 200}}
    fmt.println(len(a))
}
"#,
        ),
        vec!["HYB-E6002"],
        "elided struct literal in [1]item with overflowing i8 field must reject HYB-E6002"
    );
}

#[test]
fn rejects_const_overflow_in_elided_struct_fields_across_map_nested_and_generic_slots() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct item { b u8 }
fn main() {
    m := map[string]item{"a": {b: 300}}
    fmt.println(len(m))
}
"#,
        ),
        vec!["HYB-E6002"],
        "elided struct literal in map value slot with overflowing u8 field must reject HYB-E6002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct item { b u8 }
fn main() {
    m := map[item]int{{b: 300}: 1}
    fmt.println(len(m))
}
"#,
        ),
        vec!["HYB-E6002"],
        "elided struct literal in map key slot with overflowing u8 field must reject HYB-E6002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct box_item[t] { v t }
fn main() {
    xs := [][]box_item[u8]{[{v: 300}]}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E6002"],
        "elided generic struct literal inside nested elided slice must reject HYB-E6002"
    );
}

#[test]
fn rejects_float_to_int_truncation_and_accepts_in_range_elided_struct_fields() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct item { n int }
fn main() {
    xs := []item{{n: 2.5}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E6002"],
        "float constant 2.5 in int field of elided struct literal must reject HYB-E6002"
    );

    assert!(
        codes(
            r#"import "fmt"
struct item {
    b u8
    n i8
}
fn main() {
    xs := []item{{b: 255, n: -128}}
    ys := []*item{{b: 0, n: 127}}
    m := map[string]item{"k": {b: 42, n: 7}}
    fmt.println(len(xs), len(ys), len(m))
}
"#,
        )
        .is_empty(),
        "in-range constants in elided struct literal fields must pass with 0 diagnostics"
    );
}
