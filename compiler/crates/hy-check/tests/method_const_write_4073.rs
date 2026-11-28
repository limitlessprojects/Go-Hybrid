//! Regression tests for #4073:
//! Method calls with value or pointer receivers (including zero-field structs,
//! `(&o).method(...)` address-of receiver calls, method-value aliases, and
//! calls inside closures) must enforce T22 const-write checks (`HYB-E3006`).
//! Read-only methods on `const` arguments must continue to be accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("inline.hy", src).expect("must parse");
    check_file(&file)
}

fn codes(src: &str) -> Vec<String> {
    check(src).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn value_receiver_method_const_arg_write_rejected_e3006() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
fn (o ops) clobber(m map[string]int) {
    m["a"] = 99
}
fn main() {
    var o ops
    o.clobber(seed)
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3006"]);
}

#[test]
fn pointer_receiver_method_const_arg_write_rejected_e3006() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
fn (o *ops) clobber(m map[string]int) {
    m["a"] = 99
}
fn main() {
    var o ops
    o.clobber(seed)
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3006"]);
}

#[test]
fn addr_of_receiver_with_disjoint_method_names_rejected_e3006() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
struct benign {}
fn (o *ops) clobber(m map[string]int) {
    m["a"] = 99
}
fn (b *benign) clobber(m map[string]int) {
}
fn main() {
    var o ops
    (&o).clobber(seed)
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3006"]);
}

#[test]
fn method_value_alias_const_arg_write_rejected_e3006() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
fn (o ops) clobber(m map[string]int) {
    m["a"] = 99
}
fn main() {
    var o ops
    f := o.clobber
    f(seed)
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3006"]);
}

#[test]
fn generic_receiver_empty_struct_method_const_arg_write_rejected_e3006() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops[t] {}
fn (o ops[t]) clobber(m map[string]int) {
    m["a"] = 99
}
fn main() {
    var o ops[int]
    o.clobber(seed)
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3006"]);
}

#[test]
fn associated_fn_and_generic_fn_const_arg_write_rejected_e3006() {
    let assoc_src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
fn ops.clobber(m map[string]int) {
    m["a"] = 99
}
fn main() {
    ops.clobber(seed)
}
"#;
    assert_eq!(codes(assoc_src), vec!["HYB-E3006"]);

    let generic_src = r#"
const seed = map[string]int{"a": 1}
fn clobber[t](m map[string]int) {
    m["a"] = 99
}
fn main() {
    clobber[int](seed)
}
"#;
    assert_eq!(codes(generic_src), vec!["HYB-E3006"]);
}

#[test]
fn closure_body_method_const_arg_write_rejected_e3006() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
fn (o ops) clobber(m map[string]int) {
    m["a"] = 99
}
fn main() {
    cb := fn() {
        var o ops
        o.clobber(seed)
    }
    cb()
}
"#;
    assert!(codes(src).contains(&"HYB-E3006".to_string()));
}

#[test]
fn read_only_method_on_const_arg_accepted() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
fn (o ops) inspect(m map[string]int) int {
    return m["a"]
}
fn main() {
    var o ops
    _ = o.inspect(seed)
    _ = (&o).inspect(seed)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "unexpected diagnostics: {:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty());
}
