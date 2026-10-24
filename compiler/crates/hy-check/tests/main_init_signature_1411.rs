use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

#[test]
fn rejects_main_with_parameters_issue_1411() {
    let r = check("fn main(a int) {}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for parameterized main, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_init_with_parameters_issue_1411() {
    let r = check("fn init(a int) {}\nfn main() {}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for parameterized init, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_init_with_result_issue_1411() {
    let r = check("fn init() int {\n    return 0\n}\nfn main() {}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for result-bearing init, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn accepts_valid_main_init_and_methods_issue_1411() {
    let r_empty = check("fn init() {}\nfn main() {}\n");
    assert_eq!(r_empty.verdict, Verdict::Accept, "{:?}", r_empty.diagnostics);

    let r_main_err = check("fn main() error {\n    return nil\n}\n");
    assert_eq!(
        r_main_err.verdict,
        Verdict::Accept,
        "{:?}",
        r_main_err.diagnostics
    );

    let r_methods = check(
        "struct svc {}\n\
         fn (s svc) init(a int) int { return a }\n\
         fn (s svc) main(a int) int { return a }\n\
         fn main() {\n\
             s := svc{}\n\
             _ = s.init(1) + s.main(2)\n\
         }\n",
    );
    assert_eq!(
        r_methods.verdict,
        Verdict::Accept,
        "{:?}",
        r_methods.diagnostics
    );
}
