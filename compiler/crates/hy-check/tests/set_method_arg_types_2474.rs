//! Regression tests for #2474: `set[T]` method calls (`.insert(v)`,
//! `.contains(v)`, `.remove(v)`) and the `delete(s, v)` builtin must check
//! `v` against the set's element type `T` (SPEC §2.3 T9). A mismatched
//! argument used to pass `hyb check` with zero diagnostics and then fail
//! `go build` (GUARANTEES Row 13, rule `BUILDABLE`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_reject_with(src: &str, code: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected rejection with {code}, but accepted: {:?}\n{src}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == code),
        "expected {code} diagnostic code, got: {:?}\n{src}",
        res.diagnostics
    );
}

fn assert_reject_e1002(src: &str) {
    assert_reject_with(src, "HYB-E1002");
}

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected acceptance, but rejected with: {:?}\n{src}",
        res.diagnostics
    );
}

// ---- reject: mismatched element argument (the #2474 reproductions) ----

#[test]
fn insert_string_into_set_int_rejects() {
    assert_reject_e1002("fn main() {\n    s := set[int]{}\n    _ = s.insert(\"x\")\n}\n");
}

#[test]
fn insert_statement_form_rejects() {
    assert_reject_e1002("fn main() {\n    s := set[int]{}\n    s.insert(\"x\")\n}\n");
}

#[test]
fn contains_string_on_set_int_rejects() {
    assert_reject_e1002("fn main() {\n    s := set[int]{}\n    _ = s.contains(\"x\")\n}\n");
}

#[test]
fn delete_string_from_set_int_rejects() {
    assert_reject_with(
        "fn main() {\n    s := set[int]{}\n    delete(s, \"x\")\n}\n",
        "HYB-E2005",
    );
}

#[test]
fn remove_string_from_set_int_rejects() {
    assert_reject_e1002("fn main() {\n    s := set[int]{}\n    s.remove(\"x\")\n}\n");
}

// ---- reject: other element shapes and binding forms ----

#[test]
fn insert_int_into_set_string_rejects() {
    assert_reject_e1002("fn main() {\n    s := set[string]{}\n    _ = s.insert(1)\n}\n");
}

#[test]
fn insert_typed_binding_mismatch_rejects() {
    assert_reject_e1002(
        "fn main() {\n    s := set[int]{}\n    v := \"x\"\n    _ = s.insert(v)\n}\n",
    );
}

#[test]
fn set_param_contains_mismatch_rejects() {
    assert_reject_e1002(
        "fn has(s set[string]) bool {\n    return s.contains(3)\n}\nfn main() {\n    _ = has(set[string]{})\n}\n",
    );
}

#[test]
fn var_declared_set_insert_mismatch_rejects() {
    assert_reject_e1002("fn main() {\n    var s set[bool]\n    _ = s.insert(1)\n}\n");
}

#[test]
fn struct_elem_set_insert_wrong_struct_rejects() {
    assert_reject_e1002(
        "struct a { x int }\nstruct b { y int }\nfn main() {\n    s := set[a]{}\n    _ = s.insert(b{y: 1})\n}\n",
    );
}

#[test]
fn aliased_elem_set_insert_mismatch_rejects() {
    assert_reject_e1002(
        "type node_id = int\nfn main() {\n    s := set[node_id]{}\n    _ = s.insert(\"n1\")\n}\n",
    );
}

#[test]
fn struct_field_set_insert_mismatch_rejects() {
    assert_reject_e1002(
        "struct graph { seen set[int] }\nfn main() {\n    g := graph{seen: set[int]{}}\n    _ = g.seen.insert(\"x\")\n}\n",
    );
}

#[test]
fn delete_on_set_param_mismatch_rejects() {
    assert_reject_with(
        "fn drop(s set[string]) {\n    delete(s, 7)\n}\nfn main() {\n    drop(set[string]{})\n}\n",
        "HYB-E2005",
    );
}

// ---- reject: wrong argument count ----

#[test]
fn insert_with_no_args_rejects() {
    assert_reject_e1002("fn main() {\n    s := set[int]{}\n    _ = s.insert()\n}\n");
}

#[test]
fn contains_with_two_args_rejects() {
    assert_reject_e1002("fn main() {\n    s := set[int]{}\n    _ = s.contains(1, 2)\n}\n");
}

// ---- accept controls: legal set programs must stay accepted ----

#[test]
fn matching_set_ops_accept() {
    assert_accept(
        "fn main() {\n    s := set[int]{}\n    _ = s.insert(1)\n    _ = s.contains(2)\n    delete(s, 1)\n    n := 3\n    s.insert(n)\n    s.remove(n)\n    fmt.println(len(s))\n}\n",
    );
}

#[test]
fn untyped_int_into_set_float_accepts() {
    assert_accept(
        "fn main() {\n    s := set[f64]{}\n    _ = s.insert(1)\n    _ = s.contains(2.5)\n}\n",
    );
}

#[test]
fn set_any_accepts_any_value() {
    assert_accept(
        "fn main() {\n    s := set[any]{}\n    _ = s.insert(1)\n    _ = s.insert(\"x\")\n}\n",
    );
}

#[test]
fn set_of_struct_matching_accepts() {
    assert_accept(
        "struct pt {\n    x int\n    y int\n}\nfn main() {\n    s := set[pt]{}\n    _ = s.insert(pt{x: 1, y: 2})\n    _ = s.contains(pt{x: 1, y: 2})\n}\n",
    );
}

#[test]
fn aliased_elem_set_matching_accepts() {
    assert_accept(
        "type node_id = int\nfn main() {\n    s := set[node_id]{}\n    _ = s.insert(3)\n    delete(s, 3)\n}\n",
    );
}

#[test]
fn set_param_and_field_matching_accept() {
    assert_accept(
        "struct graph { seen set[string] }\nfn has(s set[string], k string) bool {\n    return s.contains(k)\n}\nfn main() {\n    g := graph{seen: set[string]{}}\n    _ = g.seen.insert(\"a\")\n    _ = has(g.seen, \"a\")\n}\n",
    );
}

#[test]
fn delete_on_map_still_accepted() {
    assert_accept(
        "fn main() {\n    m := map[string]int{}\n    m[\"a\"] = 1\n    delete(m, \"a\")\n}\n",
    );
}

#[test]
fn set_range_iteration_accepts() {
    assert_accept(
        "fn main() {\n    s := set[string]{}\n    _ = s.insert(\"a\")\n    for v in s {\n        fmt.println(v)\n    }\n}\n",
    );
}

#[test]
fn generic_elem_set_stays_fail_open() {
    assert_accept(
        "fn add[t](s set[t], v t) bool {\n    return s.insert(v)\n}\nfn main() {\n    _ = add(set[int]{}, 1)\n}\n",
    );
}

#[test]
fn user_declared_struct_named_set_keeps_its_own_methods() {
    assert_accept(
        "struct set { x int }\nfn (s set) insert(v string) bool {\n    return true\n}\nfn main() {\n    a := set{x: 1}\n    _ = a.insert(\"hello\")\n}\n",
    );
}
