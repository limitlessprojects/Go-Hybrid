//! Regression tests for #3947 (nested-block closure move propagation via
//! `closure_scoped_envs`, HYB-E3003) and #3948 (compound generic struct
//! type-argument unification, HYB-E3001 / HYB-E3003).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn nested_block_closure_sees_later_move_of_outer_block_binding_e3003() {
    let r = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    if true {
        m := map[string]int{"k": 1}
        if true {
            defer (fn() {
                m["k"] = 2
            })()
        }
        task worker(move m, done)
    }
    <-done
}
"#,
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn compound_generic_struct_type_arg_unifies_and_rejects_unmoved_spawn_e3001() {
    let r = check(
        r#"
type t = int

struct box[u] {
    val u
}

fn first_elem[t any](b box[[]t]) t {
    return b.val[0]
}

fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn main() {
    m := map[string]int{"init": 0}
    b := box[[]map[string]int]{val: []map[string]int{m}}
    done := make(chan int, 1)
    task worker(first_elem(b), done)
    m["main"] = 2
    <-done
}
"#,
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
