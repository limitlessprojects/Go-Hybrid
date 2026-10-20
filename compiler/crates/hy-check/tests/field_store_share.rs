//! Issue #392 — a store into a field (`h.m = shared`) must materialize the
//! may_share edge from the composite's root binding to every live
//! reference-bearing root on the right-hand side.
//!
//! Before the fix `rebind_assign_targets` only handled `PlaceKind::Ident`, so a
//! field store left the composite looking self-contained: `move h` reported no
//! unproven spawn and did not freeze `shared`, and the emitted Go spawned
//! `worker(h)` while `main` kept writing the same map — `fatal error:
//! concurrent map writes` (reproduced 5/5 under go1.26.9).

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
fn field_store_alias_freezes_source_on_move_e3003_issue_392() {
    // The reported shape: `shared` is stored into the field, the composite is
    // moved into a task, and `main` keeps writing `shared` afterwards. The
    // store must create the edge, so the post-spawn write is an alias write
    // through a frozen value.
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn worker(h holder) {\n\
           h.m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           var h holder\n\
           h.m = shared\n\
           task worker(move h)\n\
           shared[\"main\"]++\n\
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
fn field_store_alias_freezes_source_on_read_e3003_issue_392() {
    // Same edge, read form: no write after the spawn, only a READ of the
    // source binding. The freeze covers every live alias, so this rejects
    // under the current (corpus-19) reading of O5.2.
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn worker(h holder) {\n\
           h.m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           var h holder\n\
           h.m = shared\n\
           task worker(move h)\n\
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
fn field_store_fresh_literal_stays_accept_issue_392() {
    // Positive control: a FRESH literal in the field shares no live storage.
    // The fix must stay narrow and keep this accepted.
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn worker(h holder) {\n\
           h.m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           var h holder\n\
           h.m = map[string]int{}\n\
           task worker(move h)\n\
           fmt.println(\"worker spawned\")\n\
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
fn scalar_field_store_stays_accept_issue_392() {
    // The guard: a field store of a scalar never creates a may_share edge, so
    // the ordinary `svc.replicas = n` form is untouched.
    let r = check(
        "struct svc {\n\
           replicas int\n\
           tag string\n\
         }\n\
         fn main() {\n\
           var s svc\n\
           s.replicas = 3\n\
           s.tag = \"scaled\"\n\
           fmt.println(\"{s.replicas} {s.tag}\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
