#[test]
fn defer_mutating_named_results_regression_2319() {
    // Repro 1: defer mutating named result causes return sharing and move freeze violation (HYB-E3003)
    let repro1 = r#"
struct holder {
    s []int
}

fn dup() (a holder, b holder) {
    x := holder{s: []int{1}}
    defer fn() {
        a = x
    }()
    b = x
    return
}

fn worker(h holder, done chan int) {
    for i in 0..20000 {
        h.s[0] = i
    }
    done <- 1
}

fn main() {
    h1, h2 := dup()
    done := make(chan int, 1)
    task worker(move h1, done)
    for j in 0..20000 {
        h2.s[0] = j
    }
    _ = <-done
}
"#;
    let r1 = hy_check::check_file(&hy_syntax::parse_file("repro1.hy", repro1).unwrap());
    assert_eq!(r1.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r1.diagnostics);
    assert!(
        r1.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003: {:?}",
        r1.diagnostics
    );

    // Repro 1 variant: explicit return overwritten by defer still shares return pairs
    let repro1_explicit = r#"
struct holder {
    s []int
}

fn dup() (a holder, b holder) {
    x := holder{s: []int{1}}
    defer fn() {
        a = x
    }()
    return holder{s: []int{99}}, x
}

fn worker(h holder, done chan int) {
    for i in 0..20000 {
        h.s[0] = i
    }
    done <- 1
}

fn main() {
    h1, h2 := dup()
    done := make(chan int, 1)
    task worker(move h1, done)
    for j in 0..20000 {
        h2.s[0] = j
    }
    _ = <-done
}
"#;
    let r1_exp = hy_check::check_file(&hy_syntax::parse_file("repro1_exp.hy", repro1_explicit).unwrap());
    assert_eq!(r1_exp.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r1_exp.diagnostics);
    assert!(
        r1_exp.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003: {:?}",
        r1_exp.diagnostics
    );

    // Repro 2: defer mutating named result with file-level const composite rejects HYB-E3006
    let repro2 = r#"
struct holder { s []int }
const shared_const = holder{s: []int{0}}

fn leak_const() (a holder) {
    defer fn() { a = shared_const }()
    return holder{s: []int{1}}
}

fn worker(id int, done chan int) {
    h := leak_const()
    for i in 0..20000 { h.s[0] = id + i }
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    task worker(100, done)
    task worker(200, done)
    _ = <-done
    _ = <-done
}
"#;
    let r2 = hy_check::check_file(&hy_syntax::parse_file("repro2.hy", repro2).unwrap());
    assert_eq!(r2.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r2.diagnostics);
    assert!(
        r2.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006: {:?}",
        r2.diagnostics
    );

    // Repro 3: defer storing local borrow into named result on explicit return rejects HYB-E3104
    let repro3 = r#"
type int_ptr = *int

fn escape_ptr(fallback *int) (out int_ptr) {
    x := 42
    defer fn() { out = &x }()
    return fallback
}
"#;
    let r3 = hy_check::check_file(&hy_syntax::parse_file("repro3.hy", repro3).unwrap());
    assert_eq!(r3.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r3.diagnostics);
    assert!(
        r3.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104: {:?}",
        r3.diagnostics
    );

    // Repro 3 variant: bare return with defer storing local borrow rejects HYB-E3104
    let repro3_bare = r#"
type int_ptr = *int

fn escape_ptr_bare() (out int_ptr) {
    x := 42
    defer fn() { out = &x }()
    return
}
"#;
    let r3_bare = hy_check::check_file(&hy_syntax::parse_file("repro3_bare.hy", repro3_bare).unwrap());
    assert_eq!(r3_bare.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r3_bare.diagnostics);
    assert!(
        r3_bare.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104: {:?}",
        r3_bare.diagnostics
    );
}
