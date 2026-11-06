//! Regression tests for `emit_guard.rs` (`declare_method`):
//!   1. Rejecting methods declared on non-local foreign types (`*http.request`) with `HYB-E2007`.
//!   2. Rejecting methods that share a name with a field of the receiver struct (`HYB-E1003`).
//!   3. Accepting methods declared on zero-field structs that shadow primitive names (`struct byte {}`).

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
fn rejects_methods_on_foreign_types_and_field_method_name_collisions() {
    assert_eq!(
        codes(
            r#"import "fmt"
import "net/http"
fn (r *http.request) custom() int {
    return 1
}
fn main() {
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E2007"],
        "method on foreign type `*http.request` must reject with HYB-E2007"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct box_val {
    foo int
}
fn (b box_val) foo() int {
    return 1
}
fn main() {
    b := box_val{foo: 1}
    fmt.println(b.foo)
}
"#,
        ),
        vec!["HYB-E1003"],
        "method sharing a name with a field on `box_val` must reject with HYB-E1003"
    );
}

#[test]
fn accepts_methods_on_zero_field_structs_shadowing_primitive_names() {
    assert!(
        codes(
            r#"import "fmt"
struct byte {}
fn (b byte) tag() int {
    return 1
}
fn main() {
    b := byte{}
    fmt.println(b.tag())
}
"#,
        )
        .is_empty(),
        "method on user-declared zero-field `struct byte {{}}` must pass with 0 diagnostics"
    );
}
