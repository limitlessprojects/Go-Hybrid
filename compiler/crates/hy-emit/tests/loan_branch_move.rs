//! #546: an O8 slot loan whose last use is inside an `if` that also spawns
//! or sends using the loan's base map must not get a single store-back
//! after the whole `if` — on the path that moved the map, that write races
//! the new task. The loan is handed down into every branch instead.

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
    let src = "\
struct item {
    x int
}

fn main() {
    m := map[string]item{\"a\": item{x: 0}}
    n := &m[\"a\"]
    if len(m) > 5 {
        n.x = 1
    } else {
        delete(m, \"a\")
    }
    fmt.println(len(m))
}
";
    let ast = parse_file("loan_branch_delete.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(
        !between(&go, "delete(m, \"a\")", "fmt.Println(len(m))").contains("m[\"a\"] = n"),
        "a store-back after the delete would resurrect the key:\n{go}"
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
