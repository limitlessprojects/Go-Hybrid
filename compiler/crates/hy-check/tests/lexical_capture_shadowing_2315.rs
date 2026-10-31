#[test]
fn lexical_capture_shadowing_regression_2315() {
    // Repro 1: post-closure block shadowing overwrites lexical capture environment (HYB-E3001)
    let repro1 = r#"
fn worker(m map[string]int, done chan<- bool) {
    m["worker"] = 1
    done <- true
}
fn main() {
    m := map[string]int{"init": 0}
    done := make(chan bool, 1)
    if true {
        f := fn() {
            task worker(m, done)
        }
        f()
        m := 1
        _ = m
    }
    m["main"] = 2
    <-done
}
"#;
    let r1 = hy_check::check_file(&hy_syntax::parse_file("repro1.hy", repro1).unwrap());
    assert_eq!(r1.verdict, hy_check::Verdict::Reject, "expected reject for repro 1: {:?}", r1.diagnostics);
    assert!(
        r1.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001: {:?}",
        r1.diagnostics
    );

    // Repro 2: closure mutates moved map concurrently with spawned task (HYB-E3003)
    let repro2 = r#"
fn worker(m map[string]int, done chan<- bool) {
    m["worker"] = 1
    done <- true
}
fn main() {
    m := map[string]int{"init": 0}
    done := make(chan bool, 1)
    task worker(move m, done)
    if true {
        f := fn() {
            m["closure"] = 2
        }
        f()
        m := map[string]int{"fresh": 3}
        _ = m
    }
    <-done
}
"#;
    let r2 = hy_check::check_file(&hy_syntax::parse_file("repro2.hy", repro2).unwrap());
    assert_eq!(r2.verdict, hy_check::Verdict::Reject, "expected reject for repro 2: {:?}", r2.diagnostics);
    assert!(
        r2.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003: {:?}",
        r2.diagnostics
    );

    // Variant 3: move inside block after closure definition propagates move into closure (HYB-E3003)
    let repro3 = r#"
fn worker(m map[string]int, done chan<- bool) {
    m["worker"] = 1
    done <- true
}
fn main() {
    m := map[string]int{"init": 0}
    done := make(chan bool, 1)
    if true {
        f := fn() {
            m["closure"] = 2
        }
        task worker(move m, done)
        m := map[string]int{"fresh": 3}
        _ = m
        f()
    }
    <-done
}
"#;
    let r3 = hy_check::check_file(&hy_syntax::parse_file("repro3.hy", repro3).unwrap());
    assert_eq!(r3.verdict, hy_check::Verdict::Reject, "expected reject for repro 3: {:?}", r3.diagnostics);
    assert!(
        r3.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003: {:?}",
        r3.diagnostics
    );
}
