use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
}

#[test]
fn managed_non_ident_move_and_own_handle_taint_1742() {
    let r_spawn = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
           m[\"worker\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           h := holder{m: map[string]int{\"init\": 0}}\n\
           managed(\"delegate map\", \"mutex\") {\n\
             task worker(move h.m, done)\n\
           }\n\
           h.m[\"main\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r_spawn.verdict, Verdict::Reject, "{:?}", r_spawn.diagnostics);
    assert!(
        r_spawn.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "{:?}",
        r_spawn.diagnostics
    );
    assert!(
        r_spawn.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "{:?}",
        r_spawn.diagnostics
    );

    let r_send = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn main() {\n\
           ch := make(chan map[string]int, 1)\n\
           h := holder{m: map[string]int{\"init\": 0}}\n\
           managed(\"delegate map\", \"mutex\") {\n\
             ch <- move h.m\n\
           }\n\
           h.m[\"main\"] = 2\n\
         }",
    );
    assert_eq!(r_send.verdict, Verdict::Reject, "{:?}", r_send.diagnostics);
    assert!(
        r_send.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "{:?}",
        r_send.diagnostics
    );
    assert!(
        r_send.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "{:?}",
        r_send.diagnostics
    );
}
