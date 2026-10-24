use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn rangebind_preserves_outer_binding_read_in_hoisted_try_of_shadowing_exclusive_range() {
    let src = r#"import "fmt"

fn limit(n int) (int, error) {
    return n + 1, nil
}

fn run(xs []int) error {
    for i, x in xs {
        fmt.println(i)
        for x in 0..limit(x)? {
            fmt.println(x)
        }
    }
    return nil
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, x := range xs {"),
        "expected outer `x` read by hoisted `limit(x)?` before `for x := 0` to stay bound, got:\n{go}"
    );
}

#[test]
fn rangebind_preserves_outer_binding_read_in_ordered_call_before_hoisted_try_in_range_bound() {
    let src = r#"import "fmt"

fn id(n int) int {
    return n
}

fn limit(n int) (int, error) {
    return n + 1, nil
}

fn run(xs []int, ch chan int) error {
    for i, x in xs {
        fmt.println(i)
        for x in 0..(id(x) + limit(0)?) {
            fmt.println(x)
        }
    }
    select {
        x := <-ch: {
            for x in 0..limit(x)? {
                fmt.println(x)
            }
        }
        default: {}
    }
    return nil
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, x := range xs {"),
        "expected outer `x` hoisted via left-to-right `id(x)` before `for x := 0` to stay bound, got:\n{go}"
    );
    assert!(
        go.contains("case x := <-ch:"),
        "expected select receive `x` read by hoisted `limit(x)?` to stay bound, got:\n{go}"
    );
}

#[test]
fn rangebind_still_blanks_outer_binding_when_exclusive_range_bound_reads_only_inner_var() {
    let src = r#"import "fmt"

fn limit(n int) (int, error) {
    return n + 1, nil
}

fn run(xs []int) error {
    for i, x in xs {
        fmt.println(i)
        for x in 0..(x + limit(0)?) {
            fmt.println(x)
        }
    }
    return nil
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    assert!(
        go.contains("for i, _ := range xs {"),
        "expected unread outer `x` (when pure `x` stays in `for x := 0; x < (x + hyT0)`) to be blanked, got:\n{go}"
    );
}
