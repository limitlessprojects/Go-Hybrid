//! #1393: deferred closure inside nested block and shadow rebinds.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn reject(src: &str) {
    let ast = parse_file("test_1393.hy", src).expect("parses");
    let r = check_file(&ast);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject:\n{src}\n{:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "expected HYB-E3003:\n{src}\n{:?}", r.diagnostics);
}

fn accept(src: &str) {
    let ast = parse_file("test_1393.hy", src).expect("parses");
    let r = check_file(&ast);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept:\n{src}\n{:?}", r.diagnostics);
}

#[test]
fn defer_closure_in_nested_block_then_move_rejects() {
    reject(r#"import "fmt"
fn worker(owned map[string]int) { owned["k"] = 2 }
fn main() {
    shared := map[string]int{"k": 1}
    if true {
        defer (fn() { fmt.println(shared["k"]) })()
    }
    task worker(move shared)
}
"#);
}

#[test]
fn defer_shadow_rebind_in_nested_block_then_move_rejects() {
    reject(r#"import "fmt"
fn worker(owned map[string]int) { owned["k"] = 2 }
fn report(m map[string]int) { fmt.println(m["k"]) }
fn main() {
    shared := map[string]int{"k": 1}
    defer report(shared)
    if true {
        shared := map[string]int{"k": 2}
        fmt.println(shared["k"])
    }
    task worker(move shared)
}
"#);
}

#[test]
fn defer_alias_chain_17_hops_then_move_rejects() {
    reject(r#"import "fmt"
fn worker(owned map[string]int) { owned["k"] = 2 }
fn report(m map[string]int) { fmt.println(m["k"]) }
fn main() {
    shared := map[string]int{"k": 1}
    defer report(shared)
    a1 := shared
    a2 := a1
    a3 := a2
    a4 := a3
    a5 := a4
    a6 := a5
    a7 := a6
    a8 := a7
    a9 := a8
    a10 := a9
    a11 := a10
    a12 := a11
    a13 := a12
    a14 := a13
    a15 := a14
    a16 := a15
    a17 := a16
    task worker(move a17)
}
"#);
}

#[test]
fn top_level_rebind_before_move_accepts() {
    accept(r#"import "fmt"
fn worker(owned map[string]int) { owned["k"] = 2 }
fn report(m map[string]int) { fmt.println(m["k"]) }
fn main() {
    shared := map[string]int{"k": 1}
    defer report(shared)
    shared = map[string]int{"k": 2}
    task worker(move shared)
}
"#);
}
