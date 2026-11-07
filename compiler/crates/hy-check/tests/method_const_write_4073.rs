use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("should parse");
    let checked = check_file(&file);
    checked.diagnostics.into_iter().map(|d| d.code).collect()
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
fn associated_fn_const_arg_write_rejected_e3006() {
    let src = r#"
const seed = map[string]int{"a": 1}
struct ops {}
fn ops.clobber(m map[string]int) {
    m["a"] = 99
}
fn main() {
    ops.clobber(seed)
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3006"]);
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
}
"#;
    assert_eq!(codes(src), Vec::<String>::new());
}
