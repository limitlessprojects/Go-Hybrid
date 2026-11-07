//! Regression tests for `loans.rs` (`field_borrow_kind` / `check_struct_decls` /
//! `record_local_borrow_aliases`) enforcing A6 / O12 (`HYB-E3103`) and borrow-return
//! escape (`HYB-E3104`) on `set[*T]`, `set[serde_json.view]`, and generic struct
//! instantiations `box_val[*T]` / `box_val[serde_json.view]`.

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
fn rejects_struct_fields_holding_set_or_generic_struct_of_pointers_or_borrow_handles() {
    assert_eq!(
        codes(
            r#"import "fmt"
import "rust/serde_json"
struct box_val[t] {
    v t
}
struct holder {
    s_ptr set[*int]
    s_view set[serde_json.view]
    b_ptr box_val[*int]
    b_view box_val[serde_json.view]
}
fn main() {
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E3103", "HYB-E3103", "HYB-E3103", "HYB-E3103"],
        "struct fields holding `set[*int]`, `set[serde_json.view]`, `box_val[*int]`, and `box_val[serde_json.view]` must reject with HYB-E3103"
    );
}

#[test]
fn rejects_returning_generic_struct_borrow_from_multi_value_call() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct box_val[t] {
    v t
}
fn wrap_pair(p *int) (box_val[*int], int) {
    var b box_val[*int]
    _ = p
    return b, 1
}
fn leak() box_val[*int] {
    x := 10
    b, n := wrap_pair(&x)
    _ = n
    return b
}
fn main() {
    _ = leak()
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E3104"],
        "returning `b` (`box_val[*int]`) unpacked from `wrap_pair(&x)` must reject with HYB-E3104"
    );
}
