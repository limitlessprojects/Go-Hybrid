//! escape_captured_mutation_2724.rs — issue #2724 (A5, HYB-E3013), partial:
//! the two syntactically reachable vectors.
//!
//! A closure that mutates a captured binding and escapes into a foreign Go call
//! must be rejected (HYB-E3013) unless it sits inside `managed("...", "interop")`.
//! `check_escape_site` / `closure_writes_captured` missed two shapes:
//!   1. a NAMED closure binding passed as the argument (only inline `fn(){}`
//!      literals were inspected), and
//!   3. an escaping closure whose only mutation was an in-place BUILTIN on a
//!      captured container (`delete(m, k)`, `clear(m)`) — the write-walker saw
//!      only assignment place roots.
//!
//! (Vectors 2 — method calls on a foreign opaque/handle receiver — and 4 —
//! inner-block shadow masking — are NOT covered here; see the PR body.)

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn named_closure_mutating_capture_rejects_e3013() {
    let src = "\
import \"net/http\"
fn main() {
    hits := 0
    cb := fn() { hits++ }
    http.handle_func(\"/a\", cb)
    _ = hits
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E3013".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn escaping_closure_delete_on_capture_rejects_e3013() {
    let src = "\
import \"net/http\"
fn main() {
    m := map[string]int{\"k\": 1}
    http.handle_func(\"/b\", fn() { delete(m, \"k\") })
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E3013".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn escaping_closure_clear_on_capture_rejects_e3013() {
    let src = "\
import \"net/http\"
fn main() {
    m := map[string]int{\"k\": 1}
    http.handle_func(\"/c\", fn() { clear(m) })
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E3013".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn read_only_escaping_closure_stays_accepted() {
    let src = "\
import \"fmt\"
import \"net/http\"
fn main() {
    xs := []int{1}
    http.handle_func(\"/r\", fn() { fmt.println(len(xs)) })
    _ = xs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn escaping_closure_mutating_its_own_local_stays_accepted() {
    let src = "\
import \"net/http\"
fn main() {
    http.handle_func(\"/l\", fn() {
        m := map[string]int{\"k\": 1}
        delete(m, \"k\")
    })
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}
