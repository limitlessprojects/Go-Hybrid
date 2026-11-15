//! Regression tests for same-scope short variable redeclaration (`m, tag := ...`)
//! and same-scope rebind (`m = ...`) interacting with closure captures and
//! post-move freeze (`HYB-E3003`, Row 2 `MOVE-FREEZE`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("test program failed to parse: {:?}", diags));
    check_file(&file)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn nested_closure_same_scope_redecl_then_move_rejects_e3003() {
    let codes = error_codes(
        r#"
fn worker(m map[string]int, done chan int) {
    for i in 0..2000 {
        m["k"] = i
    }
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    outer := fn() {
        m := map[string]int{"init": 0}
        inner := fn() {
            for i in 0..2000 {
                m["k"] = i
            }
        }
        m, tag := map[string]int{"k": 2}, 1
        _ = tag
        task worker(move m, done)
        inner()
    }
    outer()
    <-done
}
"#,
    );
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for nested closure mutating same-scope redeclared `m` after `move m`, got {:?}",
        codes
    );
}

#[test]
fn nested_closure_same_scope_redecl_alias_and_multival_call_then_move_rejects_e3003() {
    let codes_alias = error_codes(
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    outer := fn() {
        m1 := map[string]int{"init": 0}
        m := m1
        inner := fn() {
            m["k"] = 99
        }
        m2 := map[string]int{"k": 2}
        m, tag := m2, 1
        _ = tag
        task worker(move m2, done)
        inner()
    }
    outer()
    <-done
}
"#,
    );
    assert!(
        codes_alias.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when same-scope redeclaration aliases `m` to moved `m2`, got {:?}",
        codes_alias
    );

    let codes_multival = error_codes(
        r#"
fn make_pair() (map[string]int, int) {
    return map[string]int{"k": 2}, 1
}

fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    outer := fn() {
        m := map[string]int{"init": 0}
        inner := fn() {
            m["k"] = 99
        }
        m, tag := make_pair()
        _ = tag
        task worker(move m, done)
        inner()
    }
    outer()
    <-done
}
"#,
    );
    assert!(
        codes_multival.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when multi-value call redeclares `m` in same scope before `move m`, got {:?}",
        codes_multival
    );
}

#[test]
fn flat_closure_rebind_and_same_scope_redecl_preserve_move_freeze_rejects_e3003() {
    let codes_assign = error_codes(
        r#"
fn worker(m map[string]int, done chan int) {
    for i in 0..2000 {
        m["k"] = i
    }
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := map[string]int{"init": 0}
    f := fn() {
        for i in 0..2000 {
            m["k"] = i
        }
    }
    m = map[string]int{"k": 2}
    task worker(move m, done)
    f()
    m, tag := map[string]int{"fresh": 3}, 1
    _ = m
    _ = tag
    <-done
}
"#,
    );
    assert!(
        codes_assign.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when `m` is reassigned before `move m`, called via `f()`, and redeclared via `m, tag := ...`, got {:?}",
        codes_assign
    );

    let codes_redecl = error_codes(
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := map[string]int{"init": 0}
    task worker(move m, done)
    (fn() {
        m["k"] = 99
    })()
    m, tag := map[string]int{"fresh": 3}, 1
    _ = m
    _ = tag
    <-done
}
"#,
    );
    assert!(
        codes_redecl.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when immediate closure mutates moved `m` before same-scope `m, tag := ...`, got {:?}",
        codes_redecl
    );
}

#[test]
fn inner_block_shadow_and_sibling_block_move_still_accept() {
    let file = parse_file(
        "test.hy",
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    outer := fn() {
        m := map[string]int{"outer": 0}
        inner := fn() {
            m["outer"] = 2
        }
        if true {
            m := map[string]int{"shadow": 1}
            task worker(move m, done)
        }
        inner()

        if true {
            b := map[string]int{"b1": 0}
            sub := fn() {
                _ = b["b1"]
            }
            sub()
        }
        if true {
            b := map[string]int{"b2": 0}
            task worker(move b, done)
        }
    }
    outer()
    <-done
    <-done
}
"#,
    )
    .expect("parse");
    let res = check_file(&file);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept for inner-block shadow and sibling-block move, got {:?}",
        res.diagnostics
    );
}
