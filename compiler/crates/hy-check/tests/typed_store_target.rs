//! Tests for issue #2199 -- type annotation in store target rejected.
//!
//! A type annotation in store target position (`n string = "str"`, `n int = 5`, `q.a int = 5`)
//! passed `hyb check` with 0 diagnostics and failed `go build` with `<type> (type) is not an expression`,
//! violating GUARANTEES.md Row 13 (BUILDABLE).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn local_binding_wrong_type_store_is_rejected() {
    assert_code(
        r#"
import "fmt"

fn main() {
    var n int
    n string = "str"
    fmt.println("{n}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn local_binding_matching_type_store_is_rejected() {
    assert_code(
        r#"
import "fmt"

fn main() {
    var n int
    n int = 5
    fmt.println("{n}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn struct_field_typed_store_is_rejected() {
    assert_code(
        r#"
struct point {
    a int
}

fn main() {
    var q point
    q.a int = 5
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn positive_control_plain_var_store_passes() {
    assert_ok(
        r#"
import "fmt"

fn main() {
    var n int
    n = 5
    fmt.println("{n}")
}
"#,
    );
}

#[test]
fn positive_control_plain_field_store_passes() {
    assert_ok(
        r#"
struct point {
    a int
}

fn main() {
    var q point
    q.a = 5
}
"#,
    );
}

#[test]
fn positive_control_declaration_forms_pass() {
    assert_ok(
        r#"
import "fmt"

fn main() {
    var n int = 5
    m := 5
    fmt.println("{n} {m}")
}
"#,
    );
}
