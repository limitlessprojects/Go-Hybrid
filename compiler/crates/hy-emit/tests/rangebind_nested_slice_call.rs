use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn rangebind_preserves_vars_in_nested_slice_indexed_call() {
    let src = r#"package main

import "fmt"

fn op_a(x int) int {
    return x + 1
}

fn run_nested(fns []fn(int) int, rows [][]int) {
    for i, row in rows {
        fmt.println(fns[row[i]](i))
    }
    for _, row in rows {
        for j, _ in row {
            fmt.println(fns[row[j]](1))
        }
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, row := range rows {"),
        "expected both `i` and `row` read in `fns[row[i]](i)` to remain bound, got:\n{go}"
    );
    assert!(
        go.contains("for j, _ := range row {"),
        "expected inner `j` read only inside nested index `fns[row[j]](1)` to remain bound, got:\n{go}"
    );
}

#[test]
fn rangebind_preserves_deep_nested_and_selector_indexed_calls() {
    let src = r#"package main

import "fmt"

struct config {
    slots []int
}

struct table {
    fns []fn(int) int
}

fn run_table(t table, cfgs []config, cfg_zero config, idx_zero int, outer []int, inner []int) {
    for i, cfg in cfgs {
        fmt.println(t.fns[cfg.slots[outer[inner[i]]]](1))
    }
    for slots, fns in outer {
        fmt.println(t.fns[cfg_zero.slots[idx_zero]](1))
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, cfg := range cfgs {"),
        "expected `i` and `cfg` inside `t.fns[cfg.slots[outer[inner[i]]]](1)` to stay bound, got:\n{go}"
    );
    assert!(
        go.contains("for range outer {"),
        "expected field names `.fns` and `.slots` not to count as reads of loop vars `slots` and `fns`, got:\n{go}"
    );
}
