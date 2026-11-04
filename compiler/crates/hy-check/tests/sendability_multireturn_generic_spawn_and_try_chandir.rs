use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|e| panic!("parse failed: {:?}", e));
    check_file(&ast)
}

#[test]
fn multi_return_spawn_rejects_second_result_mutable_map_e3001() {
    let r = check(
        "fn leak_map(m map[string]int) (int, map[string]int) { return 1, m }\n\
         fn worker(id int, m map[string]int) { m[\"k\"] = id }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           task worker(leak_map(m))\n\
           m[\"k\"] = 9\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3001"), "{:?}", r.diagnostics);
}

#[test]
fn multi_return_spawn_rejects_second_result_closure_e3001() {
    let r = check(
        "fn make_pair() (int, fn()) { return 1, fn() {} }\n\
         fn worker(id int, cb fn()) {}\n\
         fn main() { task worker(make_pair()) }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3001"), "{:?}", r.diagnostics);
}

#[test]
fn multi_return_spawn_rejects_second_result_unproven_type_param_e3005() {
    let r = check(
        "fn wrap_pair[t](x t) (int, t) { return 1, x }\n\
         fn worker[t](id int, val t) {}\n\
         fn run[t](x t) { task worker(wrap_pair(x)) }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3005"), "{:?}", r.diagnostics);
}

#[test]
fn multi_return_try_spawn_rejects_second_result_slice_e3001() {
    let r = check(
        "fn pair_try() (int, []int, error) { return 1, []int{1, 2}, nil }\n\
         fn worker(id int, xs []int) {}\n\
         fn run() error { task worker(pair_try()?)\n return nil }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3001"), "{:?}", r.diagnostics);
}

#[test]
fn multi_return_spawn_all_sendable_by_type_accepts() {
    let r = check(
        "fn pair_scalars() (int, string) {\n\
           return 1, \"ok\"\n\
         }\n\
         fn worker(id int, label string) {}\n\
         fn main() {\n\
           task worker(pair_scalars())\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}

#[test]
fn generic_spawn_auto_addressed_pointer_param_rejects_e3001() {
    let r = check(
        "struct stats {\n\
           n int\n\
         }\n\
         fn worker[t](s *stats, tag t) {\n\
           s.n = 1\n\
         }\n\
         fn main() {\n\
           const s = stats{n: 0}\n\
           task worker[int](s, 1)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 for auto-addressed pointer param on instantiated generic spawn, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_spawn_nil_into_instantiated_slice_param_rejects_e4001_even_in_managed() {
    let r = check(
        "fn worker[t](xs t) {}\n\
         fn main() {\n\
           managed(\"audit hatch\", \"actor\") {\n\
             task worker[[]int](nil)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E4001"),
        "expected HYB-E4001 for nil into instantiated []int param of generic spawn, got {:?}",
        r.diagnostics
    );
}

#[test]
fn generic_spawn_inferred_slice_param_rejects_nil_e4001_in_managed() {
    let r = check(
        "fn worker[t](a t, b t) {}\n\
         fn main() {\n\
           managed(\"audit hatch\", \"actor\") {\n\
             task worker([]int{1}, nil)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E4001"),
        "expected HYB-E4001 for nil into inferred []int param of generic spawn, got {:?}",
        r.diagnostics
    );
}

#[test]
fn try_unwrapped_multi_return_preserves_chan_dir_for_close_send_and_coercion() {
    let r_close = check(
        "fn open_pair(rc <-chan int) (<-chan int, int, error) {\n\
           return rc, 1, nil\n\
         }\n\
         fn run(rc <-chan int) error {\n\
           ch, _ := open_pair(rc)?\n\
           close(ch)\n\
           return nil\n\
         }",
    );
    assert_eq!(r_close.verdict, Verdict::Reject);
    assert!(
        r_close.diagnostics.iter().any(|d| d.code == "HYB-E3009"),
        "expected HYB-E3009 for close on <-chan int from ?-unwrapped multi-return, got {:?}",
        r_close.diagnostics
    );

    let r_send = check(
        "fn open_pair(rc <-chan int) (<-chan int, int, error) {\n\
           return rc, 1, nil\n\
         }\n\
         fn run(rc <-chan int) error {\n\
           ch, _ := open_pair(rc)?\n\
           ch <- 7\n\
           return nil\n\
         }",
    );
    assert_eq!(r_send.verdict, Verdict::Reject);
    assert!(
        r_send.diagnostics.iter().any(|d| d.code == "HYB-E3008"),
        "expected HYB-E3008 for send on <-chan int from ?-unwrapped multi-return, got {:?}",
        r_send.diagnostics
    );

    let r_coerce = check(
        "fn open_pair(rc <-chan int) (<-chan int, int, error) {\n\
           return rc, 1, nil\n\
         }\n\
         fn run(rc <-chan int) (chan<- int, int, error) {\n\
           var sc chan<- int\n\
           var n int\n\
           sc, n = open_pair(rc)?\n\
           return sc, n, nil\n\
         }",
    );
    assert_eq!(r_coerce.verdict, Verdict::Reject);
    assert!(
        r_coerce.diagnostics.iter().any(|d| d.code == "HYB-E3012"),
        "expected HYB-E3012 for <-chan int to chan<- int coercion via ?-unwrapped multi-return, got {:?}",
        r_coerce.diagnostics
    );
}
