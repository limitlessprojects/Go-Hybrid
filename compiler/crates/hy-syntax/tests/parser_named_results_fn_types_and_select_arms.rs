//! Regression tests for `hy-syntax` parser soundness:
//! 1. Named return parameters in `parse_results` with non-identifier types
//!    (`*T`, `[]T`, `[N]T`, `[...]T`, `chan T`, `<-chan T`, `fn(...)`),
//!    preserving the return parameter name and distinguishing `(arr [4]int, err error)`
//!    from unnamed generic return types `(box[int], error)`.
//! 2. `TypeExpr::Fn` with parenthesized multi-return types (`fn(x int) (int, error)`)
//!    in type aliases, struct fields, variable declarations, and parameter lists.
//! 3. Unbraced `select` arm body lookahead (`looks_like_select_arm_head`) when
//!    an arm body contains bare channel receives (`<-sem`) or channel sends (`out <- v`),
//!    and when subsequent arms send on field or index channels (`s.ch <- v:`, `chs[i] <- v:`).

use hy_syntax::{Decl, SelectArmKind, Stmt, TypeExpr, parse_file};

#[test]
fn named_return_params_with_pointer_slice_array_chan_and_fn_types_parse() {
    let src = r#"package main

fn make_all() (
    ptr *int,
    items []string,
    fixed [4]int,
    tx chan<- int,
    rx <-chan int,
    cb fn(x int) int,
    err error,
) {
    return
}
"#;
    let file = parse_file("test.hy", src).expect("named non-ident return types must parse");
    let Decl::Fn(f) = &file.decls[0] else {
        panic!("expected fn decl");
    };
    let names: Vec<Option<&str>> = f.results.iter().map(|r| r.name.as_deref()).collect();
    assert_eq!(
        names,
        vec![
            Some("ptr"),
            Some("items"),
            Some("fixed"),
            Some("tx"),
            Some("rx"),
            Some("cb"),
            Some("err"),
        ]
    );
    assert!(matches!(f.results[0].ty, TypeExpr::Pointer { .. }));
    assert!(matches!(f.results[1].ty, TypeExpr::Slice { .. }));
    assert!(
        matches!(f.results[2].ty, TypeExpr::Array { len: Some(_), .. }),
        "`fixed [4]int` must parse as named TypeExpr::Array, not `fixed[[4]int]`: {:?}",
        f.results[2]
    );
    assert!(matches!(f.results[3].ty, TypeExpr::Chan { .. }));
    assert!(matches!(f.results[4].ty, TypeExpr::Chan { .. }));
    assert!(matches!(f.results[5].ty, TypeExpr::Fn { .. }));
}

#[test]
fn unnamed_generic_and_builtin_return_types_stay_unnamed() {
    let src = r#"package main

struct box[t] {
    v t
}

fn fetch() (box[int], map[string]int, option[string], []int, [2]int, error) {
    return box[int]{v: 1}, map[string]int{}, none, []int{}, [2]int{1, 2}, nil
}
"#;
    let file = parse_file("test.hy", src).expect("unnamed generic return types must parse");
    let f = file.fns().next().expect("fn fetch");
    assert_eq!(f.results.len(), 6);
    for r in &f.results {
        assert!(
            r.name.is_none(),
            "expected unnamed return result, got: {r:?}"
        );
    }
    assert!(matches!(&f.results[0].ty, TypeExpr::Named { name, .. } if name == "box[int]"));
    assert!(matches!(&f.results[1].ty, TypeExpr::Map { .. }));
    assert!(matches!(&f.results[2].ty, TypeExpr::Option { .. }));
    assert!(matches!(&f.results[3].ty, TypeExpr::Slice { .. }));
    assert!(matches!(&f.results[4].ty, TypeExpr::Array { .. }));
}

#[test]
fn fn_type_with_parenthesized_multi_return_parses_in_decls_and_fields() {
    let src = r#"package main

type rpc_handler = fn(req string) (string, error)

struct service {
    handle fn(id int) (res string, err error)
}

fn run(cb fn(x int) (int, error)) {
    var local_cb fn(a int) (int, bool)
    _ = local_cb
    _ = cb
}
"#;
    let file = parse_file("test.hy", src).expect("multi-return fn types must parse");
    let Decl::TypeAlias(ta) = &file.decls[0] else {
        panic!("expected type alias");
    };
    let TypeExpr::Fn { results, .. } = &ta.ty else {
        panic!("expected TypeExpr::Fn, got: {:?}", ta.ty);
    };
    assert_eq!(results.len(), 2);

    let Decl::Struct(st) = &file.decls[1] else {
        panic!("expected struct decl");
    };
    let TypeExpr::Fn {
        results: field_results,
        ..
    } = &st.fields[0].ty
    else {
        panic!("expected field TypeExpr::Fn");
    };
    assert_eq!(field_results.len(), 2);
}

#[test]
fn unbraced_select_arms_allow_body_sends_and_recvs_and_field_send_arms() {
    let src = r#"package main

struct worker {
    done chan bool
}

fn pump(in_ch chan int, sem chan int, out chan int, chs []chan int, w worker) {
    select {
        v := <-in_ch:
            <-sem
            out <- v
        w.done <- true:
            return
        chs[0] <- 1:
            return
        default:
            return
    }
}
"#;
    let file = parse_file("test.hy", src).expect("unbraced select arms must parse cleanly");
    let f = file.fns().next().expect("fn pump");
    let body = f.body.as_ref().expect("fn body");
    let Stmt::Select { arms, .. } = &body.stmts[0] else {
        panic!("expected Stmt::Select");
    };
    assert_eq!(arms.len(), 4);
    assert!(matches!(&arms[0].kind, SelectArmKind::Recv { names, .. } if names == &["v"]));
    assert_eq!(
        arms[0].body.stmts.len(),
        2,
        "first unbraced arm must contain both `<-sem` and `out <- v` statements"
    );
    assert!(matches!(&arms[1].kind, SelectArmKind::Send { .. }));
    assert!(matches!(&arms[2].kind, SelectArmKind::Send { .. }));
    assert!(matches!(&arms[3].kind, SelectArmKind::Default));
}
