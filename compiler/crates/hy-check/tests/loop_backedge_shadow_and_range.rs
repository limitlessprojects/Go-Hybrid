use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_rules(src: &str) -> Vec<String> {
    let ast = parse_file("backedge_shadow.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    res.diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn inner_block_shadowing_does_not_erase_outer_loop_carried_move() {
    let src = r#"
package main

fn sink(m map[string]int) {
    m["worker"] = 1
}

fn main() {
    m := map[string]int{"init": 0}
    for i in 0..2 {
        if false {
            m := map[string]int{"shadow": 1}
            _ = m
        }
        m["main"] = i
        task sink(move m)
    }
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when inner block shadows m before outer m is mutated/re-moved on next iteration, got {:?}",
        codes
    );
}

#[test]
fn inner_block_shadowing_alias_does_not_unfreeze_outer_alias_on_backedge() {
    let src = r#"
package main

fn sink(m map[string]int) {
    m["worker"] = 1
}

fn main() {
    m := map[string]int{"init": 0}
    alias := m
    for i in 0..2 {
        if false {
            alias := 0
            _ = alias
        }
        alias["main"] = i
        if i == 0 {
            task sink(move m)
        }
    }
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when inner block shadows outer alias before alias writes moved map on next iteration, got {:?}",
        codes
    );
}

#[test]
fn range_loop_upper_bound_is_checked_on_backedge_after_move() {
    let src = r#"
package main

fn sink(m map[string]int) {
    m["worker"] = 1
}

fn main() {
    m := map[string]int{"limit": 2}
    for i in 0..m["limit"] {
        if i == 0 {
            task sink(move m)
        }
    }
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when range loop upper bound reads m on back-edge after move m, got {:?}",
        codes
    );
}
