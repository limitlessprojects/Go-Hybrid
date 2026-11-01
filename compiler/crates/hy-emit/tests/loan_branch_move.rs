//! #546 & #547: an O8 slot loan whose last use is inside a branching/compound
//! statement that also spawns, sends, or mutates using the loan's base map
//! must not get a single store-back after the whole construct — on the path
//! that moved or mutated the map, that write races the new task or clobbers
//! the branch's mutation. Instead, the loan is handed down into each branch.

use hy_emit::emit_go;
use hy_syntax::parse_file;

const WORKER: &str = "\
struct item {
    x int
}

fn worker(m map[string]item, done chan bool) {
    for i in 0..1000 {
        m[\"w\"] = item{x: i}
    }
    done <- true
}
";

fn emit(main_body: &str) -> String {
    let src = format!("{WORKER}\nfn main() {{\n{main_body}}}\n");
    let ast = parse_file("loan_branch_move.hy", &src).expect("parses");
    emit_go(&ast).expect("emits")
}

/// The text of `go` strictly between `from` and the next `to`.
fn between<'a>(go: &'a str, from: &str, to: &str) -> &'a str {
    let start = go.find(from).expect("has start marker") + from.len();
    let end = go[start..]
        .find(to)
        .map(|e| start + e)
        .expect("has end marker");
    &go[start..end]
}

#[test]
fn sibling_branch_move_gets_no_store_back_after_the_spawn_issue_546() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    done := make(chan bool, 1)
    n := &m[\"a\"]
    if len(m) > 5 {
        n.x = 1
        done <- true
    } else {
        task worker(move m, done)
    }
    <-done
    fmt.println(\"done\")
",
    );
    assert!(
        !between(&go, "go worker(m, done)", "<-done").contains("m[\"a\"] = n"),
        "store-back must not run after the spawn on the moved path:\n{go}"
    );
    let else_arm = between(&go, "} else {", "go worker(m, done)");
    assert!(
        else_arm.contains("m[\"a\"] = n"),
        "the moving branch stores the loan back before the spawn:\n{go}"
    );
    assert!(
        between(&go, "n.x = 1", "done <- true").contains("m[\"a\"] = n"),
        "the branch that writes the loan stores it back after its last use:\n{go}"
    );
}

#[test]
fn branch_write_is_visible_before_a_later_read_in_that_branch_issue_546() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    done := make(chan bool, 1)
    n := &m[\"a\"]
    if len(m) == 1 {
        n.x = 7
        fmt.println(m[\"a\"].x)
    } else {
        task worker(move m, done)
        <-done
    }
",
    );
    assert!(
        between(&go, "n.x = 7", "fmt.Println(m[\"a\"].x)").contains("m[\"a\"] = n"),
        "the loan write must land before the branch reads the slot:\n{go}"
    );
}

#[test]
fn branch_write_is_visible_before_later_read_even_without_sibling_move_issue_546() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    n := &m[\"a\"]
    if len(m) == 1 {
        n.x = 7
        fmt.println(m[\"a\"].x)
    }
",
    );
    assert!(
        between(&go, "n.x = 7", "fmt.Println(m[\"a\"].x)").contains("m[\"a\"] = n"),
        "the loan write must land before the branch reads the slot even without a move:\n{go}"
    );
}

#[test]
fn missing_else_gets_a_synthetic_store_back_issue_546() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    done := make(chan bool, 1)
    n := &m[\"a\"]
    if n.x == 0 {
        task worker(move m, done)
    }
    <-done
",
    );
    assert!(
        !between(&go, "go worker(m, done)", "<-done").contains("m[\"a\"] = n\n\t<-done"),
        "no store-back may follow the spawn on the taken path:\n{go}"
    );
    assert!(
        between(&go, "if n.x == 0 {", "go worker(m, done)").contains("m[\"a\"] = n"),
        "the taken branch stores back before the spawn:\n{go}"
    );
    assert!(
        between(&go, "} else {", "<-done").contains("m[\"a\"] = n"),
        "the no-branch-taken path still stores the loan back:\n{go}"
    );
}

#[test]
fn sibling_branch_delete_does_not_resurrect_the_key_issue_546() {
    for op in ["delete(m, \"a\")", "clear(m)"] {
        let src = format!(
            "\
struct item {{
    x int
}}

fn main() {{
    m := map[string]item{{\"a\": item{{x: 0}}}}
    n := &m[\"a\"]
    if len(m) > 5 {{
        n.x = 1
    }} else {{
        {op}
    }}
    fmt.println(len(m))
}}
"
        );
        let ast = parse_file("loan_branch_delete.hy", &src).expect("parses");
        let go = emit_go(&ast).expect("emits");
        assert!(
            !between(&go, op, "fmt.Println(len(m))").contains("m[\"a\"] = n"),
            "a store-back after `{op}` would resurrect the key:\n{go}"
        );
    }
}

#[test]
fn match_and_select_and_block_handoff_before_move_issue_546() {
    // 1. `match` with unguarded `some` and `none` moving `m`
    let src_match = format!(
        "{WORKER}\nfn maybe_v() option[int] {{\n    return none\n}}\nfn main() {{\n    m := map[string]item{{\"a\": item{{x: 0}}}}\n    done := make(chan bool, 1)\n    n := &m[\"a\"]\n    match maybe_v() {{\n        some(v): {{\n            n.x = v\n            done <- true\n        }}\n        none: {{\n            task worker(move m, done)\n        }}\n    }}\n    <-done\n}}\n"
    );
    let ast_match = parse_file("loan_match_move.hy", &src_match).expect("parses");
    let go_match = emit_go(&ast_match).expect("emits");
    assert!(
        !between(&go_match, "go worker(m, done)", "<-done").contains("m[\"a\"] = n"),
        "match arm move must not have store-back after spawn:\n{go_match}"
    );
    assert!(
        between(&go_match, "n.x = v", "done <- true").contains("m[\"a\"] = n"),
        "match some arm stores back before done:\n{go_match}"
    );
    assert!(
        between(&go_match, "} else {", "go worker(m, done)").contains("m[\"a\"] = n"),
        "match none arm stores back before spawn:\n{go_match}"
    );

    // 2. `select` with one arm mutating `n` and `default` moving `m`
    let go_select = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    done := make(chan bool, 1)
    ch := make(chan int, 1)
    n := &m[\"a\"]
    select {
        v := <-ch: {
            n.x = v
            done <- true
        }
        default: {
            task worker(move m, done)
        }
    }
    <-done
",
    );
    assert!(
        !between(&go_select, "go worker(m, done)", "<-done").contains("m[\"a\"] = n"),
        "select arm move must not have store-back after spawn:\n{go_select}"
    );
    assert!(
        between(&go_select, "default:", "go worker(m, done)").contains("m[\"a\"] = n"),
        "select default arm stores back before spawn:\n{go_select}"
    );

    // 3. Bare nested block `{ n.x = 1; task worker(move m, done) }`
    let go_block = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    done := make(chan bool, 1)
    n := &m[\"a\"]
    {
        n.x = 1
        task worker(move m, done)
    }
    <-done
",
    );
    assert!(
        between(&go_block, "n.x = 1", "go worker(m, done)").contains("m[\"a\"] = n"),
        "bare block must flush outer loan before spawn inside block:\n{go_block}"
    );
    assert!(
        !between(&go_block, "go worker(m, done)", "<-done").contains("m[\"a\"] = n"),
        "bare block must not flush outer loan after spawn:\n{go_block}"
    );
}

#[test]
fn if_without_a_boundary_keeps_the_single_store_back_after_it() {
    // Control: no spawn/send/delete of the base inside the `if` — the
    // lowering is unchanged (one store-back after the `if`).
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    n := &m[\"a\"]
    if len(m) > 5 {
        n.x = 1
    }
    fmt.println(len(m))
",
    );
    assert_eq!(
        go.matches("m[\"a\"] = n").count(),
        1,
        "expected exactly one store-back:\n{go}"
    );
    assert!(
        between(&go, "n.x = 1", "fmt.Println(len(m))").contains("}\n\tm[\"a\"] = n"),
        "the store-back stays after the `if`:\n{go}"
    );
}

#[test]
fn alias_move_in_branch_flushes_loan_before_spawn_issue_830() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    alias := m
    done := make(chan bool, 1)
    n := &m[\"a\"]
    if len(m) == 1 {
        n.x = 7
        task worker(move alias, done)
    } else {
        done <- true
    }
    <-done
    fmt.println(\"done\")
",
    );
    assert!(
        between(&go, "n.x = 7", "go worker(alias, done)").contains("m[\"a\"] = n"),
        "store-back must flush before `go worker(alias, done)`:\n{go}"
    );
    assert!(
        !between(&go, "go worker(alias, done)", "<-done").contains("}\n\tm[\"a\"] = n"),
        "store-back must not run after the `if` once `alias` has been moved:\n{go}"
    );
}

#[test]
fn alias_delete_in_branch_does_not_resurrect_key_issue_830() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    alias := m
    n := &m[\"a\"]
    if len(m) > 5 {
        n.x = 1
    } else {
        delete(alias, \"a\")
    }
    fmt.println(len(m))
",
    );
    assert!(
        !between(&go, "delete(alias, \"a\")", "fmt.Println(len(m))").contains("m[\"a\"] = n"),
        "store-back after `delete(alias, \"a\")` would resurrect the deleted key:\n{go}"
    );
}

#[test]
fn header_use_followed_by_branch_read_flushes_before_read_issue_830() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    n := &m[\"a\"]
    n.x = 7
    if n.x == 7 {
        fmt.println(m[\"a\"].x)
    }
",
    );
    assert!(
        between(&go, "if n.x == 7 {", "fmt.Println(m[\"a\"].x)").contains("m[\"a\"] = n"),
        "header use of `n` must flush `m[\"a\"] = n` before reading `m[\"a\"].x` in the branch:\n{go}"
    );
}

#[test]
fn else_if_cond_reads_base_after_inherited_loan_flushes_first_issue_830() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    done := make(chan bool, 1)
    n := &m[\"a\"]
    n.x = 7
    if len(m) > 5 {
        n.x = 8
        done <- true
    } else if m[\"a\"].x == 7 {
        task worker(move m, done)
    } else {
        done <- true
    }
    <-done
    fmt.println(\"done\")
",
    );
    assert!(
        between(&go, "} else {", "if m[\"a\"].x == 7 {").contains("m[\"a\"] = n"),
        "else branch must flush `m[\"a\"] = n` before evaluating `else if m[\"a\"].x == 7`:\n{go}"
    );
    assert!(
        !between(&go, "go worker(m, done)", "<-done").contains("m[\"a\"] = n"),
        "store-back must not follow `go worker(m, done)`:\n{go}"
    );
}

#[test]
fn for_loop_zero_trip_preserves_pre_loop_write_issue_830() {
    let go = emit(
        "    m := map[string]item{\"a\": item{x: 0}}
    n := &m[\"a\"]
    n.x = 7
    for i in 0..0 {
        n.x = 8
        fmt.println(m[\"a\"].x)
    }
    fmt.println(m[\"a\"].x)
",
    );
    assert!(
        between(&go, "n.x = 7", "for i := 0; i < 0; i++").contains("m[\"a\"] = n"),
        "pre-loop write `n.x = 7` must be flushed before a potentially 0-trip `for` loop:\n{go}"
    );
    assert!(
        between(&go, "n.x = 8", "fmt.Println(m[\"a\"].x)").contains("m[\"a\"] = n"),
        "in-loop write `n.x = 8` must still be flushed before reading `m[\"a\"].x` inside the loop:\n{go}"
    );
}

