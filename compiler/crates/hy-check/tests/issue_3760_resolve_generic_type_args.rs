//! Regression test for issue #3760:
//! hy-check/resolve: validate bracketed generic type arguments on calls,
//! new(set/option), and array-length type args (HYB-E1001).
//!
//! compiler/crates/hy-check/src/resolve.rs previously dropped HYB-E1001
//! undefined-name validation on bracketed generic type arguments in four positions:
//! 1. Top-level generic function calls (id[undef_ty](1)), multi-argument generic calls,
//!    nested generic type arguments, and slice/pointer type arguments.
//! 2. Selector calls on methods and packages (h.wrap[undef_ty](1), pkg.fn[undef_ty](1)).
//! 3. new(set[undef_ty]) and new(option[undef_ty]) where run_type_arg_expr returned early.
//! 4. Array-length expressions inside generic type instantiations (var b box[[undef_len]int]).

use hy_syntax::parse_file;
use hy_check::check_file;

#[test]
fn issue_3760_reproduction_emits_hyb_e1001() {
    let src = r#"
struct box[t] {
    v t
}
fn id[t](x t) t {
    return x
}
fn pair[a, b](x a, y b) a {
    return x
}
fn bad() {
    _ = id[undef_ty](1)
    _ = pair[int, undef_ty](1, 2)
    _ = new(set[undef_set_elem])
    var b box[[undef_len]int]
    _ = b
}
"#;
    let file = parse_file("issue_3760_repro.hy", src).expect("parse ok");
    let res = check_file(&file);

    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("`undef_ty` is not available here")),
        "expected HYB-E1001 for undef_ty in id[undef_ty](1), got: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("`undef_set_elem` is not available here")),
        "expected HYB-E1001 for undef_set_elem in new(set[undef_set_elem]), got: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("`undef_len` is not available here")),
        "expected HYB-E1001 for undef_len in box[[undef_len]int], got: {:?}",
        res.diagnostics
    );
}

#[test]
fn issue_3760_all_positions_coverage() {
    let src = r#"
package main

import "strings"

struct box[t] {
    v t
}

struct helper {}

fn (h helper) wrap[t](x t) t {
    return x
}

fn id[t](x t) t {
    return x
}

fn pair[a, b](x a, y b) a {
    return x
}

fn test_pos1_top_level_and_nested() {
    b := box[int]{v: 1}
    _ = id[undef_ty1](1)
    _ = pair[int, undef_ty2](1, 2)
    _ = id[box[undef_ty3]](b)
    _ = pair[int, []undef_ty4](1, nil)
    _ = pair[int, *undef_ty5](1, nil)
}

fn test_pos2_selectors(h helper) {
    _ = h.wrap[undef_ty6](1)
    _ = strings.clone[undef_ty7]("hello")
}

fn test_pos3_new() {
    _ = new(set[undef_ty8])
    _ = new(option[undef_ty9])
}

fn test_pos4_array_len_in_generic_types() {
    var b box[[undef_len1]int]
    _ = b
    s := set[[undef_len2]int]{}
    _ = s
    m := make([]box[[undef_len3]int], 1)
    _ = m
}

fn test_valid_cases_no_false_positives(h helper) {
    b := box[int]{v: 1}
    _ = id[int](1)
    _ = pair[int, string](1, "ok")
    _ = id[box[int]](b)
    _ = pair[int, []string](1, nil)
    _ = pair[int, *int](1, nil)
    _ = h.wrap[int](1)
    _ = strings.clone[string]("ok")
    _ = new(set[int])
    _ = new(option[string])
    var b2 box[[10]int]
    _ = b2
    s2 := set[[5]int]{}
    _ = s2
    m2 := make([]box[[8]int], 1)
    _ = m2
}
"#;
    let file = parse_file("issue_3760_all.hy", src).expect("parse ok");
    let res = check_file(&file);

    for i in 1..=9 {
        let name = format!("`undef_ty{}`", i);
        assert!(
            res.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains(&name)),
            "missing HYB-E1001 for {}",
            name
        );
    }
    for i in 1..=3 {
        let name = format!("`undef_len{}`", i);
        assert!(
            res.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains(&name)),
            "missing HYB-E1001 for {}",
            name
        );
    }

    let valid_e1001 = res.diagnostics.iter().filter(|d| {
        d.code == "HYB-E1001" && (d.message.contains("`int`") || d.message.contains("`string`") || d.message.contains("`box`") || d.message.contains("`10`") || d.message.contains("`5`") || d.message.contains("`8`"))
    }).collect::<Vec<_>>();
    assert!(valid_e1001.is_empty(), "unexpected false positive HYB-E1001: {:?}", valid_e1001);
}
