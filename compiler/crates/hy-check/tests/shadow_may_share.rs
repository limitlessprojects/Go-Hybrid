use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_error_rule(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .and_then(|d| d.tier0_rule.clone())
}

#[test]
fn shadowed_may_share_peer_freezes_when_holder_moves_in_block_e3003_issue_195() {
    // Issue #195: `h := holder{m: m}` shares storage with `m` via `may_share`.
    // Shadowing `m` inside an `if` block and moving `h` inside that block must
    // freeze the outer `m` when the block exits.
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn worker(h holder, done chan<- bool) {\n\
           for i in 0..10 {\n\
             h.m[\"k\"] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           h := holder{m: m}\n\
           done := make(chan bool, 1)\n\
           if true {\n\
             m := 42\n\
             fmt.println(m)\n\
             task worker(move h, done)\n\
           }\n\
           for i in 0..10 {\n\
             m[\"k\"] = i\n\
           }\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn shadowed_holder_freezes_when_map_moves_in_block_e3003_issue_195() {
    // Symmetric form: `h` is shadowed inside the block while `m` is moved;
    // using `h.m` after the block must also reject with HYB-E3003.
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
           m[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           h := holder{m: m}\n\
           done := make(chan bool, 1)\n\
           if true {\n\
             h := 42\n\
             fmt.println(h)\n\
             task worker(move m, done)\n\
           }\n\
           h.m[\"k\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}
