//! make_directional_channel_issue_1804.rs — issue #1804: Go §Making slices,
//! maps and channels requires "The channel type must be bidirectional";
//! `go build` rejects `make(chan<- T)` / `make(<-chan T)` at compile time.
//! `classify_make_target` ignored `ChanDir`, so `hyb check` accepted a
//! directional `make` target with 0 diagnostics and the emitted Go did not
//! build. Fix: carry the direction through `MakeTargetKind::Chan(ChanDir)`
//! and reject any non-`Both` target with HYB-E2005 (existing code — no new
//! registry entry).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn make_receive_only_type_is_rejected() {
    let r = check("fn main() {\n    _ = make(<-chan int, 5)\n}\n");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "make(<-chan int) must fire HYB-E2005, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn make_send_only_type_is_rejected() {
    let r = check("fn main() {\n    _ = make(chan<- int)\n}\n");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "make(chan<- int) must fire HYB-E2005, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn make_bidirectional_size_still_accepted() {
    let r = check("fn main() {\n    ch := make(chan int, 5)\n    _ = ch\n}\n");
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E2005"),
        "bidirectional make must stay accepted, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn make_bidirectional_zero_size_still_accepted() {
    let r = check("fn main() {\n    ch := make(chan int)\n    _ = ch\n}\n");
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E2005"),
        "unbuffered bidirectional make must stay accepted, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn directional_make_inside_let_assignment_is_rejected() {
    // Same gate must apply when the result is bound, not just discarded.
    let r = check("fn main() {\n    ch := make(<-chan int, 10)\n    _ = ch\n}\n");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "bound directional make must fire HYB-E2005, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_and_slice_targets_unaffected() {
    let r = check(
        "fn main() {\n    m := make(map[string]int{})\n    s := make([]int, 2)\n    _ = m\n    _ = s\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E2005"),
        "map/slice make targets must stay accepted, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
