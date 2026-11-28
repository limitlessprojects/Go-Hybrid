//! time_duration_untyped_const_init.rs — `time.duration` (Go `time.Duration`),
//! `time.month` and `time.weekday` are named integer types: an untyped
//! constant initialises them and they take arithmetic and comparison, so
//! `var d time.duration = 3` must check OK (it builds). A typed integer of
//! another width must still be rejected (HYB-E1002).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn prog(pre: &str, body: &str) -> String {
    format!("import \"time\"\n{}\nfn main() {{\n{}\n}}\n", pre, body)
}

#[test]
fn untyped_constants_and_arithmetic_on_time_integer_types_are_accepted() {
    let cases = [
        ("", "    var d time.duration = 3\n    time.sleep(d)\n"),
        ("", "    var d time.duration\n    d = 3\n    time.sleep(d)\n"),
        ("", "    const k = 3\n    var d time.duration = k\n    time.sleep(d)\n"),
        ("", "    var d time.duration = 3 * 2\n    time.sleep(d)\n"),
        ("", "    var d time.duration = 3\n    d += 1\n    if d > 2 {\n        time.sleep(d)\n    }\n"),
        ("", "    var a time.duration = 1\n    var b time.duration = 2\n    time.sleep(a + b)\n"),
        ("", "    var d time.duration = 3\n    time.sleep(d * 2)\n"),
        ("", "    time.sleep(take(3))\n"),
        ("", "    ds := []time.duration{1, 2}\n    time.sleep(ds[0])\n"),
        ("", "    var m time.month = 3\n    _ = m\n"),
        ("", "    var w time.weekday = 3\n    _ = w\n"),
        ("struct st {\n    d time.duration\n}\n", "    s := st{d: 3}\n    time.sleep(s.d)\n"),
    ];
    for (pre, body) in cases {
        let pre = format!("{}fn take(d time.duration) time.duration {{\n    return d\n}}\n", pre);
        let src = prog(&pre, body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for:\n{}\n{:?}", src, r.diagnostics);
    }
}

#[test]
fn typed_integers_and_wrong_kinds_still_reject_on_time_integer_types() {
    let bodies = [
        "    var n int64 = 3\n    var d time.duration = n\n    time.sleep(d)\n",
        "    n := 3\n    var d time.duration = n\n    time.sleep(d)\n",
        "    var d time.duration = 3\n    n := 3\n    time.sleep(d * n)\n",
        "    var d time.duration = 3\n    var n int64 = d\n    _ = n\n",
        "    var m time.month = 3\n    var n int = m\n    _ = n\n",
        "    var d time.duration = 1.5\n    time.sleep(d)\n",
    ];
    for body in bodies {
        let src = prog("", body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
    }
}
