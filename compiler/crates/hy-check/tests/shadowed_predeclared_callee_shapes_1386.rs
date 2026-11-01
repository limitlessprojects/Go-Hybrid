//! Regression tests for #1386:
//! Package-level declarations shadowing predeclared universe or prelude callees and
//! types (`min`, `max`, `len`, `cap`, `some`, `string`, etc.) must resolve before
//! hardcoded builtin return shapes, `some(...)` option-wrapper sendability shortcuts,
//! and `A3_PURE_FNS` purity exemptions (`HYB-E3001` / `HYB-E3006`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn shadowed_builtin_min_returning_slice_rejected_at_spawn() {
    let r = check(
        "fn min(s []int) []int {\n\
           return s\n\
         }\n\
         fn worker(view []int, done chan<- int) {\n\
           view[0] = 1\n\
           done <- 1\n\
         }\n\
         fn main() {\n\
           s := []int{0}\n\
           done := make(chan int, 1)\n\
           task worker(min(s), done)\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3001");
}

#[test]
fn shadowed_string_type_alias_conversion_rejected_at_spawn() {
    let r = check(
        "type string = []byte\n\
         fn worker(view string, done chan<- int) {\n\
           view[0] = 1\n\
           done <- 1\n\
         }\n\
         fn main() {\n\
           raw := []byte{0}\n\
           aliased := string(raw)\n\
           done := make(chan int, 1)\n\
           task worker(aliased, done)\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3001");
}

#[test]
fn shadowed_some_returning_map_rejected_at_spawn() {
    let r = check(
        "fn some(n int) map[string]int {\n\
           return map[string]int{\"k\": n}\n\
         }\n\
         fn worker(m map[string]int, done chan<- int) {\n\
           m[\"k\"] = 2\n\
           done <- 1\n\
         }\n\
         fn main() {\n\
           done := make(chan int, 1)\n\
           task worker(some(1), done)\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3001");
}

#[test]
fn shadowed_len_mutating_const_slice_rejected_with_e3006() {
    let r = check(
        "const shared = []int{0}\n\
         fn len(s []int) {\n\
           s[0] = 1\n\
         }\n\
         fn main() {\n\
           len(shared)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn shadowed_len_transitive_const_mutation_rejected_with_e3006() {
    let r = check(
        "const shared = []int{0}\n\
         fn len(s []int) {\n\
           s[0] = 1\n\
         }\n\
         fn relay(s []int) {\n\
           len(s)\n\
         }\n\
         fn main() {\n\
           relay(shared)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn unshadowed_builtins_and_primitive_shadow_remain_accepted() {
    let unshadowed = check(
        "const shared = []int{0}\n\
         fn worker(n int, done chan<- int) {\n\
           done <- n\n\
         }\n\
         fn main() {\n\
           done := make(chan int, 1)\n\
           n := min(len(shared), 2)\n\
           task worker(n, done)\n\
           <-done\n\
         }",
    );
    assert_eq!(
        unshadowed.verdict,
        Verdict::Accept,
        "{:?}",
        unshadowed.diagnostics
    );

    let primitive_shadow = check(
        "fn min(s []int) int {\n\
           return 0\n\
         }\n\
         fn worker(n int, done chan<- int) {\n\
           done <- n\n\
         }\n\
         fn main() {\n\
           s := []int{1}\n\
           done := make(chan int, 1)\n\
           task worker(min(s), done)\n\
           <-done\n\
         }",
    );
    assert_eq!(
        primitive_shadow.verdict,
        Verdict::Accept,
        "{:?}",
        primitive_shadow.diagnostics
    );
}
