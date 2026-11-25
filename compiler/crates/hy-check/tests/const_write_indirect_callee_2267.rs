//! Regression tests for #2267: T22 (`HYB-E3006`) const-write checks on indirect callees
//! and struct-field function pointer parameters.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let parsed = parse_file("test.hy", src).expect("parse should succeed");
    let out = check_file(&parsed);
    out.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn rejects_const_write_via_indexed_callee_and_returned_fn() {
    let src = r#"
package main

fn mutate_map(m map[string]int) {
    m["k"] = 99
}

fn get_mutator() fn(map[string]int) {
    return mutate_map
}

fn main() {
    const cm = map[string]int{"k": 1}
    let ops = []fn(map[string]int){mutate_map}
    ops[0](cm)
    get_mutator()(cm)
}
"#;
    let c = codes(src);
    let e3006_count = c.iter().filter(|x| x.as_str() == "HYB-E3006").count();
    assert!(
        e3006_count >= 2,
        "expected >=2 HYB-E3006 diagnostics for indexed and returned-fn calls on const map, got: {:?}",
        c
    );
}

#[test]
fn rejects_const_write_via_struct_field_fn_auto_addr() {
    let src = r#"
package main

struct cell {
    v int
}

struct mut_box {
    f fn(p *cell)
}

fn bump(p *cell) {
    p.v = 99
}

fn main() {
    const x = cell{v: 7}
    let b = mut_box{f: bump}
    b.f(x)
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|x| x == "HYB-E3006"),
        "expected HYB-E3006 for auto-addressed const passed to struct-field fn(p *cell), got: {:?}",
        c
    );
}
