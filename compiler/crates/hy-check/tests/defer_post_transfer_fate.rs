use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parse");
    let checked = check_file(&file);
    checked.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn reject_defer_before_consuming_call_in_block_or_via_alias() {
    let src = r#"import "rust/flate2"

fn inspect(w flate2.writer) {}

fn main() {
    if true {
        w := flate2.new_writer()
        defer inspect(w)
        out := w.finish()
        _ = out
    }
    w2 := flate2.new_writer()
    alias := w2
    defer inspect(w2)
    w2 = flate2.new_writer()
    _ = w2
    out2 := flate2.finish(alias)
    _ = out2
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3003", "HYB-E3003"]);
}

#[test]
fn reject_defer_before_select_move_send() {
    let src = r#"import "fmt"

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn main() {
    ch := make(chan map[string]int, 2)
    if true {
        shared := map[string]int{"k": 1}
        defer report(shared)
        select {
        ch <- move shared: {
        }
        }
    }
    top := map[string]int{"k": 2}
    alias := top
    defer report(top)
    top = map[string]int{"k": 3}
    fmt.println(top["k"])
    select {
    ch <- move alias: {
    }
    }
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3003", "HYB-E3003"]);
}

#[test]
fn reject_defer_before_expr_move_and_shadowed_or_match_peer_moves() {
    let src = r#"import "fmt"

fn worker(owned map[string]int) {
    owned["k"] = 2
}

fn take(owned map[string]int) {
    owned["k"] = 3
}

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn main() {
    if true {
        shared := map[string]int{"k": 1}
        defer report(shared)
        take(move shared)
    }
    shadow := 1
    _ = shadow
    if true {
        shadow := map[string]int{"k": 1}
        alias := shadow
        defer report(alias)
        alias = map[string]int{"k": 2}
        fmt.println(alias["k"])
        task worker(move shadow)
    }
    m1 := map[string]int{"k": 1}
    defer report(m1)
    match some(m1) {
    some(m_alias): {
        m1 = map[string]int{"k": 2}
        fmt.println(m1["k"])
        task worker(move m_alias)
    }
    none: {
    }
    }
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3003", "HYB-E3003", "HYB-E3003"]);
}
