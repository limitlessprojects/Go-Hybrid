//! Issue #2382: `check_deferred_uses` (`HYB-E3003`) catches `select` move-sends
//! (`ch <- move shared:`) and expression/control-flow moves (`consume(move shared)`).

fn codes(src: &str) -> Vec<String> {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_check::check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn defer_before_select_move_send_and_expr_move_rejected() {
    let src = r#"import "fmt"

fn worker(ch chan map[string]int) {
    m := <-ch
    m["k"] = 99
}

fn consume(owned map[string]int) {
    owned["k"] = 77
}

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn via_select() {
    ch := make(chan map[string]int, 1)
    task worker(ch)
    shared := map[string]int{"k": 1}
    defer report(shared)
    select {
        ch <- move shared:
    }
}

fn via_expr() {
    shared := map[string]int{"k": 1}
    defer report(shared)
    consume(move shared)
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3003", "HYB-E3003"]);
}