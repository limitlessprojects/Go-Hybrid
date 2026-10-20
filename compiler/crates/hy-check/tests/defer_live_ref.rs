//! Issue #415 — a live `defer` keeps its arguments alive until function exit.
//!
//! `defer report(shared)` evaluates `shared` at the defer site but CALLS at
//! function exit, i.e. after `task worker(move shared)` transferred ownership.
//! Treating the argument as an ordinary read at the defer statement let the
//! transfer through, and the emitted Go raced:
//! `fatal error: concurrent map read and map write` (5/5 under go1.26.9).

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
fn deferred_argument_moved_into_task_rejects_e3003_issue_415() {
    let r = check(
        "fn report(m map[string]int) int {\n\
           return m[\"task\"]\n\
         }\n\
         fn worker(m map[string]int) {\n\
           m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           defer report(shared)\n\
           task worker(move shared)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    // Logged amendment (pr-421 landing): the landed machinery is #421's
    // value-precise deferred-binding rule, which rejects these shapes with
    // the generic use-after-move HYB-E3003 (A1b) rather than #418's
    // defer-specific wording — the two mechanisms collide (double report),
    // so the wording pin is relaxed to the verdict pin: REJECT + E3003 + A1b.
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "expected a use-after-move HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn deferred_closure_capture_still_rejects_issue_415() {
    // The pre-existing form: a deferred CLOSURE that captures the map. It is
    // already rejected through the capture-freeze path, and must stay so.
    let r = check(
        "fn worker(m map[string]int) {\n\
           m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           show := fn() {\n\
             fmt.println(\"{shared[\"task\"]}\")\n\
           }\n\
           defer show()\n\
           task worker(move shared)\n\
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
fn deferred_alias_guards_the_moved_origin_e3003_issue_415() {
    // The deferred call names the ALIAS while the move transfers the ORIGIN.
    // Keying on the moved name alone misses this; the check must use the
    // alias / may-share component.
    let r = check(
        "fn report(m map[string]int) int {\n\
           return m[\"task\"]\n\
         }\n\
         fn worker(m map[string]int) {\n\
           m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           alias := shared\n\
           defer report(alias)\n\
           task worker(move shared)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    // Logged amendment (pr-421 landing): the landed machinery is #421's
    // value-precise deferred-binding rule, which rejects these shapes with
    // the generic use-after-move HYB-E3003 (A1b) rather than #418's
    // defer-specific wording — the two mechanisms collide (double report),
    // so the wording pin is relaxed to the verdict pin: REJECT + E3003 + A1b.
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "expected a use-after-move HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn rebind_does_not_forget_the_captured_storage_e3003_issue_438() {
    // #438: `defer report(shared)` captured the map `shared` held THEN.
    // Rebinding the name moves the name on, not the value — `alias` still
    // holds it, so moving `alias` races the deferred read.
    let r = check(
        "fn report(m map[string]int) int {\n\
           return m[\"task\"]\n\
         }\n\
         fn worker(m map[string]int) {\n\
           m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           alias := shared\n\
           defer report(shared)\n\
           shared = map[string]int{}\n\
           task worker(move alias)\n\
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
fn plain_rebind_without_defer_stays_accept_issue_438() {
    // Control: rebinding on its own is ordinary code.
    let r = check(
        "fn main() {\n\
           shared := map[string]int{}\n\
           alias := shared\n\
           shared = map[string]int{}\n\
           alias[\"main\"]++\n\
           fmt.println(\"{len(shared)}\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn deferred_argument_moved_into_channel_rejects_e3003_issue_415() {
    // The send-side twin: `ch <- move shared` transfers ownership, and the
    // deferred call still reads the map at function exit.
    let r = check(
        "fn report(m map[string]int) int {\n\
           return m[\"task\"]\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           ch := make(chan map[string]int, 1)\n\
           defer report(shared)\n\
           ch <- move shared\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    // Logged amendment (pr-421 landing): the landed machinery is #421's
    // value-precise deferred-binding rule, which rejects these shapes with
    // the generic use-after-move HYB-E3003 (A1b) rather than #418's
    // defer-specific wording — the two mechanisms collide (double report),
    // so the wording pin is relaxed to the verdict pin: REJECT + E3003 + A1b.
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "expected a use-after-move HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn deferred_argument_for_another_map_stays_accept_issue_415() {
    // Positive control: the deferred call reads a DIFFERENT map, so moving
    // `shared` is fine. The check must key on the named binding, not on the
    // mere presence of a `defer`.
    let r = check(
        "fn report(m map[string]int) int {\n\
           return m[\"task\"]\n\
         }\n\
         fn worker(m map[string]int) {\n\
           m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           stats := map[string]int{}\n\
           defer report(stats)\n\
           task worker(move shared)\n\
           fmt.println(\"{len(stats)}\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn move_without_defer_stays_accept_issue_415() {
    // Second control: no defer at all — the plain owned move is untouched.
    let r = check(
        "fn worker(m map[string]int) {\n\
           m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           task worker(move shared)\n\
           fmt.println(\"spawned\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
