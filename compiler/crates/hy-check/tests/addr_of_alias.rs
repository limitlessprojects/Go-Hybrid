//! Issue #435 — `&h` laundered through a call must stay a live alias of `h`.
//!
//! `collect_ref_roots` had arms for `Try`, `Slice`, `Field`, `Index`, `Call`
//! and the literals, but none for address-of (in either AST spelling). So for
//! `p := keep(&h)` the argument contributed no root, `p` was never linked to
//! `h`, and `move h` froze nothing `p` could still reach — accepted with 0
//! diagnostics while the emitted Go raced 5/5.

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
fn addr_of_through_call_freezes_on_move_e3003_issue_435() {
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn keep(h *holder) *holder {\n\
           return h\n\
         }\n\
         fn worker(h holder) {\n\
           h.m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           var h holder\n\
           h.m = map[string]int{}\n\
           p := keep(&h)\n\
           task worker(move h)\n\
           p.m[\"main\"]++\n\
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
fn direct_addr_of_still_rejects_issue_435() {
    // The pre-existing twin: `p := &h` without the call. It already rejected
    // and must keep rejecting.
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
           p := &h\n\
           task worker(move h)\n\
           p.m[\"main\"]++\n\
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
fn read_only_addr_of_call_stays_accept_issue_435() {
    // The over-rejection guard: `read(&h)` takes the address but returns a
    // scalar, so nothing aliases `h` afterwards and the move is safe.
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn read(h *holder) int {\n\
           return len(h.m)\n\
         }\n\
         fn worker(h holder) {\n\
           h.m[\"task\"]++\n\
         }\n\
         fn main() {\n\
           var h holder\n\
           h.m = map[string]int{}\n\
           n := read(&h)\n\
           task worker(move h)\n\
           fmt.println(\"{n}\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn addr_of_call_without_move_stays_accept_issue_435() {
    // Second control: no boundary at all — taking the address is ordinary code.
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn keep(h *holder) *holder {\n\
           return h\n\
         }\n\
         fn main() {\n\
           var h holder\n\
           h.m = map[string]int{}\n\
           p := keep(&h)\n\
           p.m[\"main\"]++\n\
           fmt.println(\"done\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
