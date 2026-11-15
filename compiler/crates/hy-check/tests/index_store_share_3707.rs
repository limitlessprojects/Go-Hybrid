//! Issue #3707 — an index store (`list[0] = shared`, `arr[0] = shared`) must
//! materialize the `may_share` edge from the container's root binding to every
//! live reference-bearing root on the right-hand side — including when the
//! source binding is simultaneously rebound or fanned out in a parallel
//! assignment (`shared, list[0] = map[string]int{}, shared` or
//! `p, list[0] = map[string]int{}, p`).

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
fn index_store_slice_alias_freezes_source_on_move_e3003_issue_3707() {
    let r = check(
        "fn worker(l []map[string]int) {\n\
           l[0][\"k\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{\"k\": 0}\n\
           list := make([]map[string]int, 1)\n\
           list[0] = shared\n\
           task worker(move list)\n\
           shared[\"k\"]++\n\
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
fn index_store_slice_alias_freezes_source_on_read_e3003_issue_3707() {
    let r = check(
        "fn worker(l []map[string]int) {\n\
           l[0][\"k\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{\"k\": 0}\n\
           list := make([]map[string]int, 1)\n\
           list[0] = shared\n\
           task worker(move list)\n\
           fmt.println(\"{len(shared)}\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn index_store_array_and_helper_freeze_source_on_move_e3003_issue_3707() {
    let r_arr = check(
        "fn worker(a [1]map[string]int) {\n\
           a[0][\"k\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{\"k\": 0}\n\
           var arr [1]map[string]int\n\
           arr[0] = shared\n\
           task worker(move arr)\n\
           shared[\"k\"]++\n\
         }",
    );
    assert_eq!(r_arr.verdict, Verdict::Reject, "{:?}", r_arr.diagnostics);
    assert!(
        r_arr.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for array index store, got {:?}",
        r_arr.diagnostics
    );

    let r_helper = check(
        "fn put(l []map[string]int, m map[string]int) {\n\
           l[0] = m\n\
         }\n\
         fn worker(l []map[string]int) {\n\
           l[0][\"k\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{\"k\": 0}\n\
           list := make([]map[string]int, 1)\n\
           put(list, shared)\n\
           task worker(move list)\n\
           shared[\"k\"]++\n\
         }",
    );
    assert_eq!(r_helper.verdict, Verdict::Reject, "{:?}", r_helper.diagnostics);
    assert!(
        r_helper.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for helper index store, got {:?}",
        r_helper.diagnostics
    );
}

#[test]
fn index_store_after_parallel_rebind_freezes_surviving_peer_e3003_issue_3707() {
    let r = check(
        "fn worker(l []map[string]int) {\n\
           l[0][\"k\"]++\n\
         }\n\
         fn main() {\n\
           var shared = map[string]int{\"k\": 0}\n\
           alias := shared\n\
           list := make([]map[string]int, 1)\n\
           shared, list[0] = map[string]int{}, shared\n\
           task worker(move list)\n\
           alias[\"k\"]++\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on surviving peer alias after parallel index store, got {:?}",
        r.diagnostics
    );
}

#[test]
fn index_store_after_parallel_rebind_rejects_param_laundering_e3004_issue_3707() {
    let r = check(
        "fn worker(l []map[string]int) {\n\
           l[0][\"k\"]++\n\
         }\n\
         fn relay(p map[string]int) {\n\
           list := make([]map[string]int, 1)\n\
           p, list[0] = map[string]int{}, p\n\
           task worker(move list)\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{\"k\": 0}\n\
           relay(shared)\n\
           shared[\"k\"]++\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when parameter is stored into index via parallel rebind, got {:?}",
        r.diagnostics
    );
}

#[test]
fn index_store_parallel_fanout_freezes_coassigned_target_e3003_issue_3707() {
    let r = check(
        "fn worker(l []map[string]int) {\n\
           l[0][\"k\"]++\n\
         }\n\
         fn main() {\n\
           var shared = map[string]int{\"k\": 0}\n\
           var b map[string]int\n\
           list := make([]map[string]int, 1)\n\
           shared, b, list[0] = map[string]int{}, shared, shared\n\
           task worker(move list)\n\
           b[\"k\"]++\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on co-assigned target b after parallel index store, got {:?}",
        r.diagnostics
    );
}

#[test]
fn index_store_fresh_literal_and_rebound_source_stay_accept_issue_3707() {
    let r = check(
        "fn worker(l []map[string]int) {\n\
           l[0][\"k\"]++\n\
         }\n\
         fn main() {\n\
           var shared = map[string]int{\"k\": 0}\n\
           list := make([]map[string]int, 1)\n\
           shared, list[0] = map[string]int{}, shared\n\
           task worker(move list)\n\
           shared[\"fresh\"] = 1\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().all(|d| d.severity != Severity::Error),
        "unexpected error diagnostics: {:?}",
        r.diagnostics
    );
}

#[test]
fn index_store_in_closure_after_parallel_rebind_rejects_const_write_e3006_issue_3707() {
    let r = check(
        "const c = map[string]int{\"k\": 0}\n\
         fn main() {\n\
           f := fn() {\n\
             var x = map[string]int{\"k\": 0}\n\
             alias := x\n\
             list := make([]map[string]int, 1)\n\
             x, list[0] = map[string]int{}, x\n\
             var bridge = [2]map[string]int{alias, c}\n\
             _ = bridge\n\
             list[0][\"k\"] = 1\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 on const write through closure parallel index store, got {:?}",
        r.diagnostics
    );
}
