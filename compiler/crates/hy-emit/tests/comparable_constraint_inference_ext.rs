//! Regression tests for `hy-emit` inferring `comparable` type parameter
//! constraints from:
//!   1. `new(set[t])` and `make(map[k]v)` inside generic function bodies.
//!   2. `==` and `!=` equality comparisons on generic type parameters.
//!   3. Methods on generic structs (`fn (b box_val[t]) to_set() set[t]`).
//!   4. Transitive generic struct fields and parameters (`struct outer[t] { in inner_set[t] }`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parses");
    emit_go(&file).expect("emits")
}

#[test]
fn infers_comparable_from_new_set_and_equality_in_generic_fns() {
    let go = emit(
        r#"package main

import "fmt"

fn count_new_set[t]() int {
    s := new(set[t])
    return len(s)
}

fn same[t](a t, b t) bool {
    return a == b
}

fn main() {
    fmt.println(count_new_set[int](), same(1, 1))
}
"#,
    );
    assert!(
        go.contains("func count_new_set[t comparable]() int {")
            && go.contains("func same[t comparable](a t, b t) bool {"),
        "expected `count_new_set` and `same` to infer `[t comparable]`:\n{go}"
    );
}

#[test]
fn infers_comparable_from_methods_and_transitive_generic_structs() {
    let go = emit(
        r#"package main

import "fmt"

struct box_val[t] {
    v t
}

fn (b box_val[t]) to_set() set[t] {
    var s set[t]
    s.insert(b.v)
    return s
}

struct outer_box[t] {
    inner box_val[t]
}

fn use_outer[t](o outer_box[t]) int {
    return len(o.inner.to_set())
}

fn main() {
    var o outer_box[int]
    fmt.println(use_outer(o))
}
"#,
    );
    assert!(
        go.contains("type box_val[t comparable] struct {")
            && go.contains("type outer_box[t comparable] struct {")
            && go.contains("func use_outer[t comparable](o outer_box[t]) int {"),
        "expected method and transitive generic struct/fn to infer `[t comparable]`:\n{go}"
    );
}
