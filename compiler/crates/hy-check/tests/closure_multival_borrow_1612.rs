#[test]
fn closure_multival_borrow_and_named_result_slot_loans_1612() {
    // Repro 1 from issue #1612: multi-value closure define `p, ok := pair_ptr(&x)` tracks borrow
    // and rejects returning `p` with HYB-E3104 (loan escape).
    let repro1 = r#"
fn pair_ptr(p *int) (*int, bool) {
    return p, true
}

fn main() {
    f := fn() *int {
        x := 1
        p, ok := pair_ptr(&x)
        _ = ok
        return p
    }
    _ = f()
}
"#;
    let r1 = hy_check::check_file(&hy_syntax::parse_file("repro1.hy", repro1).unwrap());
    assert_eq!(r1.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r1.diagnostics);
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3104"), "{:?}", r1.diagnostics);

    // Repro 2: closure assigning map slot loan to named result with mutation before bare return rejects HYB-E3102
    let repro2 = r#"
fn main() {
    c := fn() (out *int) {
        m := map[int]int{1: 10, 2: 20}
        out = &m[1]
        m[2] = 30
        return
    }
    _ = c()
}
"#;
    let r2 = hy_check::check_file(&hy_syntax::parse_file("repro2.hy", repro2).unwrap());
    assert_eq!(r2.verdict, hy_check::Verdict::Reject, "expected reject: {:?}", r2.diagnostics);
    assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3102"), "{:?}", r2.diagnostics);
}
