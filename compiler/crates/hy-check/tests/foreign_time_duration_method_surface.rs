//! Tests for curated foreign method surface of `time.duration` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on `time.duration` or `*time.duration`
//! rejects fail-closed with HYB-E1002, while all real Go standard library methods accept cleanly.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_time_duration_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             d time.duration\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.d.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn pointer_time_duration_phantom_method_is_rejected() {
    let bad = check(
        "fn check_ptr(d *time.duration) {\n\
             d.unknown_method()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn time_package_functions_called_as_duration_methods_are_rejected() {
    for pkg_fn in ["now", "sleep", "since", "until"] {
        let src = format!(
            "struct holder {{\n\
                 d time.duration\n\
             }}\n\
             fn main() {{\n\
                 var h holder\n\
                 h.d.{pkg_fn}()\n\
                 fmt.println(1)\n\
             }}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {pkg_fn}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {pkg_fn}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn real_time_duration_methods_accept() {
    let good = check(
        "struct holder {\n\
             d time.duration\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             _ = h.d.hours()\n\
             _ = h.d.minutes()\n\
             _ = h.d.seconds()\n\
             _ = h.d.milliseconds()\n\
             _ = h.d.microseconds()\n\
             _ = h.d.nanoseconds()\n\
             _ = h.d.string()\n\
             _ = h.d.abs()\n\
             _ = h.d.round(h.d)\n\
             _ = h.d.truncate(h.d)\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}

#[test]
fn pointer_time_duration_real_methods_accept() {
    let good = check(
        "fn check_ptr(d *time.duration) {\n\
             _ = d.seconds()\n\
             _ = d.milliseconds()\n\
             _ = d.string()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
