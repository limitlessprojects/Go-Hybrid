use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
}

#[test]
fn parenthesized_values_preserve_sendability_and_taint_checks_1743() {
    let r_spawn = check(
        "import \"sync/atomic\"\n\
         struct state {\n\
           a atomic.int64\n\
           m map[string]int\n\
         }\n\
         fn worker(opt option[*atomic.int64], done chan<- bool) {\n\
           _ = opt\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           s := state{m: map[string]int{\"k\": 1}}\n\
           task worker((some(&s.a)), done)\n\
           s.m[\"k\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r_spawn.verdict, Verdict::Reject, "{:?}", r_spawn.diagnostics);
    assert!(
        r_spawn.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "{:?}",
        r_spawn.diagnostics
    );

    let r_send = check(
        "import \"sync/atomic\"\n\
         struct state {\n\
           a atomic.int64\n\
           m map[string]int\n\
         }\n\
         fn main() {\n\
           ch := make(chan option[*atomic.int64], 1)\n\
           s := state{m: map[string]int{\"k\": 1}}\n\
           ch <- (some(&s.a))\n\
         }",
    );
    assert_eq!(r_send.verdict, Verdict::Reject, "{:?}", r_send.diagnostics);
    assert!(
        r_send.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "{:?}",
        r_send.diagnostics
    );

    let r_fresh = check(
        "struct point { x int\n y int }\n\
         fn main() {\n\
           ch := make(chan point, 1)\n\
           ch <- (point{x: 1, y: 2})\n\
         }",
    );
    assert_eq!(r_fresh.verdict, Verdict::Accept, "{:?}", r_fresh.diagnostics);
}
