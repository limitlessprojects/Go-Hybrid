//! time_duration_typed_mismatch.rs — `time.Duration` is a distinct Go type.
//! Mixing the `time` unit constants / duration constructors / `since` with a
//! typed integer (`n * time.millisecond`, `d + n`, `var n int64 = time.second`)
//! or passing a typed integer to `time.sleep`/`after`/`tick` passes `hyb check`
//! but `go build` refuses it (GUARANTEES Row 13, HYB-E1002).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn prog(body: &str) -> String {
    format!("import \"time\"\n\nfn main() {{\n{}\n}}\n", body)
}

#[test]
fn typed_integer_mixed_with_duration_rejects_e1002() {
    let bodies = [
        "    n := 5\n    time.sleep(n)\n",
        "    var n int64 = 5\n    time.sleep(n)\n",
        "    n := 5\n    time.sleep(n * 1000)\n",
        "    n := 5\n    <-time.after(n)\n",
        "    n := 5\n    _ = time.tick(n)\n",
        "    n := 5\n    d := n * time.millisecond\n    time.sleep(d)\n",
        "    n := 5\n    d := time.millisecond * n\n    time.sleep(d)\n",
        "    d := time.second\n    var n int64 = d\n    _ = n\n",
        "    d := time.second\n    n := 3\n    time.sleep(d + n)\n",
        "    s := time.since(time.now())\n    var n int64 = s\n    _ = n\n",
        "    var n int64 = 2\n    time.sleep(time.seconds(1) + n)\n",
    ];
    for body in bodies {
        let src = prog(body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn duration_arithmetic_and_conversions_stay_accepted() {
    let bodies = [
        "    n := 5\n    time.sleep(time.milliseconds(n))\n",
        "    time.sleep(100 * time.millisecond)\n",
        "    d := 2 * time.second\n    time.sleep(d)\n    time.sleep(d * 3)\n    time.sleep(d + time.millisecond)\n",
        "    n := 5\n    time.sleep(time.duration(n) * time.millisecond)\n",
        "    var f float64 = float64(time.second)\n    _ = f\n",
        "    t0 := time.now()\n    el := time.since(t0)\n    if el > time.second {\n        time.sleep(el)\n    }\n",
        "    var d time.duration = time.second\n    time.sleep(d)\n",
    ];
    for body in bodies {
        let src = prog(body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for:\n{}\n{:?}", src, r.diagnostics);
    }
}

#[test]
fn shadowed_time_name_is_not_the_package() {
    let src = "struct clock {\n    second int64\n}\n\nfn main() {\n    time := clock{second: 1}\n    var n int64 = 5\n    var m int64 = n * time.second\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
