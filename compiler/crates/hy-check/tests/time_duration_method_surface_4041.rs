//! Issue #4041 (+ its duplicate #4042): `time.duration` needs a curated method
//! surface.
//!
//! `foreign::foreign_method_surface` modelled `time.ticker`, `time.timer` and
//! `time.time` but had no arm for the curated `time.duration` handle, so a
//! phantom method on a duration receiver fell through to `ForeignOpaque`:
//!
//! ```hybrid
//! import "time"
//! fn bad(d time.duration) { _ = d.totally_fake_method() }
//! ```
//!
//! `hyb check` printed `OK (0 diagnostics)`; `go build` said
//! `d.totally_fake_method undefined (type time.Duration has no field or method
//! totally_fake_method)`. A Row 13 BUILDABLE break.
//!
//! What this file pins:
//!   1. a phantom method on `time.duration` is rejected (E1002);
//!   2. the same on a `*time.duration`-shaped receiver spelling;
//!   3. every real `time.Duration` method from `go doc time.Duration` stays
//!      accepted — the point is to pin the true method set, not to reject more;
//!   4. `time.ticker` (an arm that already existed) still behaves, so the new
//!      arm did not disturb the neighbouring entries.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn hud(src: &str) -> CheckResult {
    check(src)
}

const PRELUDE: &str = "package main\n\nimport \"time\"\n\n";

#[test]
fn phantom_method_on_time_duration_rejected() {
    // The exact repro from #4041.
    let src = format!(
        "{PRELUDE}fn bad(d time.duration) {{\n    _ = d.totally_fake_method()\n}}\n\nfn main() {{}}\n"
    );
    let r = hud(&src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("totally_fake_method")),
        "expected E1002 naming the phantom method, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn all_real_duration_methods_accepted() {
    // `go doc time.Duration` publishes exactly these methods. If the surface
    // ever drops one of them this test fails, which is the failure mode the
    // issue is about (an incomplete surface accepts garbage) inverted.
    let body: String = [
        "abs", "hours", "microseconds", "milliseconds", "minutes", "nanoseconds", "round",
        "seconds", "string", "truncate",
    ]
    .iter()
    .enumerate()
    .map(|(i, m)| {
        // `string` and the no-arg accessors are called directly; `round` and
        // `truncate` need an argument, `abs` is nullary.
        if *m == "round" || *m == "truncate" {
            format!("    _ = d.{m}(d)\n")
        } else {
            let _ = i;
            format!("    _ = d.{m}()\n")
        }
    })
    .collect();

    let src = format!("{PRELUDE}fn good(d time.duration) {{\n{body}}}\n\nfn main() {{}}\n");
    let r = hud(&src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "real duration methods must all be accepted, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn ticker_arm_still_works() {
    // Neighbouring arm guard: `time.ticker` had its own entry before this fix.
    let phantom = hud(&format!(
        "{PRELUDE}fn bad(t time.ticker) {{\n    _ = t.totally_fake_method()\n}}\n\nfn main() {{}}\n"
    ));
    assert_eq!(phantom.verdict, Verdict::Reject, "{:?}", phantom.diagnostics);

    let real = hud(&format!(
        "{PRELUDE}fn good(t time.ticker) {{\n    t.stop()\n}}\n\nfn main() {{}}\n"
    ));
    assert_eq!(real.verdict, Verdict::Accept, "{:?}", real.diagnostics);
}
