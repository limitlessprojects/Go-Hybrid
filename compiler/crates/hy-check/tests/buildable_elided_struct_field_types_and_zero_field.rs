//! Regression tests for `buildable.rs` field-value type/scalar/container
//! checks on type-elided struct literals (`{f: val}`) and unknown/duplicate
//! field checks on explicit zero-field struct literals (`struct empty {}`)
//! (`HYB-E1002`, GUARANTEES Row 13 `BUILDABLE`).

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
fn rejects_unknown_and_duplicate_fields_on_explicit_zero_field_struct_literals() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct empty {}
fn main() {
    e := empty{x: 1}
    _ = e
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E1002"],
        "unknown field on explicit zero-field struct literal must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct empty {}
fn main() {
    e := empty{x: 1, x: 2}
    _ = e
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E1002", "HYB-E1002", "HYB-E1002"],
        "duplicate and unknown fields on explicit zero-field struct literal must reject with HYB-E1002"
    );
}

#[test]
fn rejects_field_value_type_and_scalar_mismatches_in_elided_struct_literals() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct point { x int }
fn main() {
    xs := []point{{x: "not_an_int"}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "string value for int field in []point{{x: ...}} must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct point { x int64 }
fn main() {
    n := 1
    xs := []*point{{x: n}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "typed int variable for int64 field in []*point{{x: n}} must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct holder { arr [2]int }
fn main() {
    a := [1]holder{{arr: [3]int{1, 2, 3}}}
    fmt.println(len(a))
}
"#,
        ),
        vec!["HYB-E1002"],
        "sized array mismatch in [1]holder elided struct literal must reject with HYB-E1002"
    );
}

#[test]
fn rejects_field_value_mismatches_in_map_and_nested_elided_struct_literals() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct point { x int }
fn main() {
    m := map[string]point{"a": {x: "bad"}}
    fmt.println(len(m))
}
"#,
        ),
        vec!["HYB-E1002"],
        "field type mismatch in map value elided struct literal must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct point { x int }
fn main() {
    m := map[point]int{{x: "bad"}: 1}
    fmt.println(len(m))
}
"#,
        ),
        vec!["HYB-E1002"],
        "field type mismatch in map key elided struct literal must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct box_val[t] { v t }
fn main() {
    xs := [][]box_val[int]{[{v: "bad"}]}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "field type mismatch in generic elided struct literal inside nested untyped slice must reject with HYB-E1002"
    );

    assert!(
        codes(
            r#"import "fmt"
struct point { x int }
struct empty {}
fn main() {
    e := empty{}
    _ = e
    xs := []point{{x: 1}}
    ys := []*point{{x: 2}}
    m := map[string]point{"k": {x: 3}}
    fmt.println(len(xs), len(ys), len(m))
}
"#,
        )
        .is_empty(),
        "valid explicit zero-field and elided struct literals must pass with 0 diagnostics"
    );
}
