//! Issue #1742 — a non-identifier `move` (`move h.m`) inside a `managed`
//! block must not vanish from the audit or leave its root unfrozen.
//!
//! The O5.4 call was guarded by `!managed` and the value classified as Ok, so
//! the site was not counted in `sites_via_managed`, `HYB-E8001` was not
//! emitted, and `h` was not frozen: `h.m["main"] = 2` raced the spawned task
//! with 0 diagnostics.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

const PRELUDE: &str = "struct holder {\n\
       m map[string]int\n\
     }\n\
     fn worker(m map[string]int, done chan<- bool) {\n\
       m[\"worker\"] = 1\n\
       done <- true\n\
     }\n";

#[test]
fn managed_move_of_gc_field_freezes_root_and_audits_issue_1742() {
    let src = format!(
        "{PRELUDE}fn main() {{\n\
           done := make(chan bool, 1)\n\
           h := holder{{m: map[string]int{{\"init\": 0}}}}\n\
           managed(\"delegate map\", \"mutex\") {{\n\
             task worker(move h.m, done)\n\
           }}\n\
           h.m[\"main\"] = 2\n\
           <-done\n\
         }}"
    );
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E8001"),
        "expected HYB-E8001, got {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn managed_move_of_gc_field_is_counted_as_managed_site_issue_1742() {
    let src = format!(
        "{PRELUDE}fn main() {{\n\
           done := make(chan bool, 1)\n\
           h := holder{{m: map[string]int{{\"init\": 0}}}}\n\
           managed(\"delegate map\", \"mutex\") {{\n\
             task worker(move h.m, done)\n\
           }}\n\
           <-done\n\
         }}"
    );
    let r = check(&src);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E8001"),
        "managed escape must be audited, got {:?}",
        r.diagnostics
    );
    assert!(
        !r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "no use after the move, so no freeze error: {:?}",
        r.diagnostics
    );
}

#[test]
fn unmanaged_move_of_gc_field_still_rejected_e3004() {
    let src = format!(
        "{PRELUDE}fn main() {{\n\
           done := make(chan bool, 1)\n\
           h := holder{{m: map[string]int{{\"init\": 0}}}}\n\
           task worker(move h.m, done)\n\
           <-done\n\
         }}"
    );
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
}

#[test]
fn managed_move_of_whole_binding_is_unchanged() {
    let src = "fn worker(m map[string]int, done chan<- bool) {\n\
                 m[\"worker\"] = 1\n\
                 done <- true\n\
               }\n\
               fn main() {\n\
                 done := make(chan bool, 1)\n\
                 m := map[string]int{\"init\": 0}\n\
                 managed(\"delegate map\", \"mutex\") {\n\
                   task worker(move m, done)\n\
                 }\n\
                 <-done\n\
               }";
    let r = check(src);
    assert!(
        !r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "{:?}",
        r.diagnostics
    );
}
