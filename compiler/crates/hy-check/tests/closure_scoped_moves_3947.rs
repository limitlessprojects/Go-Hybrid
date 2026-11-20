use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())

.collect()
}

#[test]
fn nested_block_closure_sees_later_move_of_outer_block_binding_e3003() {
    let r = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    if true {
        m := map[string]int{"k": 1}
        if true {
            defer (fn() {
                m["k"] = 2
            })()
        }
        task worker(move m, done)
    }
    <-done
}
"#,
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
