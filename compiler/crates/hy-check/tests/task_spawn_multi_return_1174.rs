use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

#[test]
fn multi_value_call_forwarding_at_task_spawn_rejects_non_sendable_second_result() {
    let r = check(
        "fn pair(m map[string]int) (int, map[string]int) {\n\
           return 1, m\n\
         }\n\
         fn worker(n int, m map[string]int) { m[\"worker\"] = n }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           task worker(pair(m))\n\
           m[\"main\"] = 2\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3001");
}

#[test]
fn multi_value_try_call_forwarding_at_task_spawn_rejects_non_sendable_second_result() {
    let r = check(
        "fn pair_try(m map[string]int) (int, map[string]int, error) {\n\
           return 1, m, nil\n\
         }\n\
         fn worker(n int, m map[string]int) { m[\"worker\"] = n }\n\
         fn run() error {\n\
           m := map[string]int{\"init\": 0}\n\
           task worker(pair_try(m)?)\n\
           m[\"main\"] = 2\n\
           return nil\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3001");
}

#[test]
fn multi_value_method_and_field_call_forwarding_at_task_spawn_rejects_non_sendable_second_result() {
    let r = check(
        "struct box {}\n\
         fn (b box) pair(m map[string]int) (int, map[string]int) {\n\
           return 1, m\n\
         }\n\
         fn worker(n int, m map[string]int) { m[\"worker\"] = n }\n\
         fn main() {\n\
           b := box{}\n\
           m := map[string]int{\"init\": 0}\n\
           task worker(b.pair(m))\n\
           m[\"main\"] = 2\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3001");

    let r_field = check(
        "struct holder { f fn(map[string]int) (int, map[string]int) }\n\
         fn worker(n int, m map[string]int) { m[\"worker\"] = n }\n\
         fn run(h holder) {\n\
           m := map[string]int{\"init\": 0}\n\
           task worker(h.f(m))\n\
           m[\"main\"] = 2\n\
         }",
    );
    assert_eq!(r_field.verdict, Verdict::Reject, "{:?}", r_field.diagnostics);
    assert_eq!(r_field.diagnostics[0].code, "HYB-E3001");
}

#[test]
fn multi_value_call_forwarding_at_task_spawn_fails_closed_on_unproven_second_result() {
    let r = check(
        "fn pair_unproven(x any) (int, any) {\n\
           return 1, x\n\
         }\n\
         fn worker(n int, x any) {}\n\
         fn run(x any) {\n\
           task worker(pair_unproven(x))\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3005");
}

#[test]
fn multi_value_call_forwarding_at_task_spawn_accepts_when_all_results_sendable() {
    let r = check(
        "fn pair_ok() (int, string) {\n\
           return 1, \"ok\"\n\
         }\n\
         fn worker(n int, s string) {}\n\
         fn main() {\n\
           task worker(pair_ok())\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
