#[test]
fn defer_capture_moved_via_post_defer_match_arm_alias_is_rejected_e3003() {
    let src = r#"
enum packet {
    data(map[string]int)
    empty
}

fn consume(m map[string]int) {}
fn inspect(p packet) {}

fn main() {
    let p = packet.data(map[string]int{"k": 1})
    defer inspect(p)
    match p {
        packet.data(alias) => {
            consume(move alias)
        }
        packet.empty => {
            return
        }
    }
}
"#;
    let parsed = hy_syntax::parse_file("test.hy", src).expect("parse");
    let checked = hy_check::check_file(&parsed);
    assert!(
        checked.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when post-defer match arm binding 'alias' sharing storage with deferred 'p' is moved, got: {:?}",
        checked.diagnostics
    );
}

#[test]
fn defer_capture_rebound_in_post_defer_match_arm_to_unrelated_subject_allows_move() {
    let src = r#"
enum boxed {
    item(map[string]int)
}

fn consume(m map[string]int) {}
fn observe(m map[string]int) {}

fn main() {
    let x = map[string]int{"a": 1}
    let unrelated = boxed.item(map[string]int{"b": 2})
    defer observe(x)
    match unrelated {
        boxed.item(x) => {
            consume(move x)
        }
    }
}
"#;
    let parsed = hy_syntax::parse_file("test.hy", src).expect("parse");
    let checked = hy_check::check_file(&parsed);
    assert!(
        !checked.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "unexpected HYB-E3003 after match arm rebound 'x' to non-watched 'unrelated': {:?}",
        checked.diagnostics
    );
}

#[test]
fn defer_watched_binding_moved_via_index_or_slice_projection_is_rejected_e3003() {
    let src = r#"
fn take_row(r []int) {}
fn read_matrix(grid [][]int) {}

fn main() {
    let grid = [][]int{[]int{1, 2}, []int{3, 4}}
    defer read_matrix(grid)
    take_row(move grid[0])
}
"#;
    let parsed = hy_syntax::parse_file("test.hy", src).expect("parse");
    let checked = hy_check::check_file(&parsed);
    assert!(
        checked.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when moving index projection 'move grid[0]' of deferred binding 'grid', got: {:?}",
        checked.diagnostics
    );
}

#[test]
fn defer_watched_binding_moved_via_parenthesized_channel_send_is_rejected_e3003() {
    let src = r#"
fn observe(m map[string]int) {}

fn main() {
    let ch = make(chan map[string]int, 1)
    let m = map[string]int{"k": 42}
    defer observe(m)
    ch <- move (m)
}
"#;
    let parsed = hy_syntax::parse_file("test.hy", src).expect("parse");
    let checked = hy_check::check_file(&parsed);
    assert!(
        checked.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when sending parenthesized 'ch <- move (m)' after defer, got: {:?}",
        checked.diagnostics
    );
}
