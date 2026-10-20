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

#[test]
fn inner_iterate_header_evaluates_outer_binding_before_loop_var_shadows_on_backedge() {
    let src = r#"
package main

fn sink(m map[string]int) {
    m["worker"] = 1
}

fn main() {
    m := map[string]int{"init": 1}
    alias := m
    for i in 0..2 {
        for _, m in m {
            _ = m
        }
        task sink(move alias)
        alias = map[string]int{"fresh": i}
    }
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when inner for-in header reads outer moved m before loop var m shadows it on back-edge, got {:?}",
        codes
    );
}

#[test]
fn inner_range_header_evaluates_outer_binding_before_loop_var_shadows_on_backedge() {
    let src = r#"
package main

fn sink(m map[string]int) {
    m["worker"] = 1
}

fn main() {
    m := map[string]int{"limit": 1}
    alias := m
    for i in 0..2 {
        for m in 0..m["limit"] {
            _ = m
        }
        task sink(move alias)
        alias = map[string]int{"limit": i}
    }
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when inner range header reads outer moved m before loop var m shadows it on back-edge, got {:?}",
        codes
    );
}

#[test]
fn block_self_shadowing_struct_with_map_freezes_outer_when_inner_moves() {
    let src = r#"
package main

struct holder {
    m map[string]int
}

fn sink(h holder) {
    h.m["worker"] = 1
}

fn main() {
    h := holder{m: map[string]int{"a": 1}}
    if true {
        h := h
        task sink(move h)
    }
    h.m["main"] = 2
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when inner self-shadowed struct with map is moved and outer struct is later mutated, got {:?}",
        codes
    );
}

#[test]
fn block_self_shadowing_map_assigned_to_outer_peer_retains_share_edge_after_scope_pop() {
    let src = r#"
package main

fn sink(m map[string]int) {
    m["worker"] = 1
}

fn main() {
    m := map[string]int{"a": 1}
    outer2 := map[string]int{}
    if true {
        m := m
        outer2 = m
    }
    task sink(move m)
    outer2["main"] = 2
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when outer2 is assigned from block-shadowed m := m and outer m is later moved, got {:?}",
        codes
    );
}

#[test]
fn iterate_loop_var_stored_into_outer_container_propagates_non_owned_origin_across_scope_pop() {
    let src = r#"
package main

fn sink(mm map[string]map[string]int) {
    mm["a"]["worker"] = 1
}

fn main() {
    ms := []map[string]int{map[string]int{"k": 1}}
    mm := map[string]map[string]int{}
    for _, p in ms {
        mm["a"] = p
    }
    task sink(move mm)
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 when outer map stores range element from ms and is moved while ms retains element, got {:?}",
        codes
    );
}

#[test]
fn user_method_returning_map_records_may_share_with_receiver() {
    let src = r#"
package main

struct holder {
    m map[string]int
}

fn (h holder) get_map() map[string]int {
    return h.m
}

fn sink(h holder) {
    h.m["worker"] = 1
}

fn main() {
    h := holder{m: map[string]int{"a": 1}}
    alias := h.get_map()
    task sink(move h)
    alias["main"] = 2
}
"#;
    let codes = error_rules(src);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when user method returns interior map and receiver is subsequently moved, got {:?}",
        codes
    );
}

#[test]
fn multi_value_self_redefine_and_shadow_preserves_share_and_alias_edges() {
    let src1 = r#"
package main

struct holder {
    m map[string]int
}

fn alias_pair(h holder) (holder, bool) {
    return h, true
}

fn sink(h holder) {
    h.m["worker"] = 1
}

fn main() {
    h := holder{m: map[string]int{"a": 1}}
    h2 := h
    h, ok := alias_pair(h)
    _ = ok
    task sink(move h)
    h2.m["main"] = 2
}
"#;
    let codes1 = error_rules(src1);
    assert!(
        codes1.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when multi-value self-redefine h, ok := alias_pair(h) preserves h2 share edge, got {:?}",
        codes1
    );

    let src2 = r#"
package main

struct holder {
    m map[string]int
}

fn alias_pair(h holder) (holder, bool) {
    return h, true
}

fn sink(h holder) {
    h.m["worker"] = 1
}

fn main() {
    h := holder{m: map[string]int{"a": 1}}
    if true {
        h, ok := alias_pair(h)
        _ = ok
        task sink(move h)
    }
    h.m["main"] = 2
}
"#;
    let codes2 = error_rules(src2);
    assert!(
        codes2.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when block self-shadowing h, ok := alias_pair(h) moves inner h and outer h is later mutated, got {:?}",
        codes2
    );
}


