//! Regression (#4042): `foreign_method_surface` must reject phantom methods
//! on `time.duration` (both annotated bindings/fields and duration constructors)
//! while accepting all real Go `time.Duration` methods.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn phantom_time_duration_methods_rejected_e1002() {
    let struct_field = check(
        "import \"time\"\n\
         import \"fmt\"\n\
         struct holder {\n\
             d time.duration\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.d.totally_fake()\n\
             fmt.println(\"done\")\n\
         }\n",
    );
    assert_eq!(struct_field.verdict, Verdict::Reject, "{:?}", struct_field.diagnostics);
    assert!(
        struct_field
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("time.duration")),
        "expected HYB-E1002 on time.duration, got {:?}",
        struct_field.diagnostics
    );

    let ctor_var = check(
        "import \"time\"\n\
         fn main() {\n\
             var d = time.seconds(5)\n\
             d.totally_fake()\n\
         }\n",
    );
    assert_eq!(ctor_var.verdict, Verdict::Reject, "{:?}", ctor_var.diagnostics);
    assert!(
        ctor_var.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 on constructor time.seconds, got {:?}",
        ctor_var.diagnostics
    );

    let parsed = check(
        "import \"time\"\n\
         fn main() {\n\
             d := time.parse_duration(\"5s\")?\n\
             d.bogus_method()\n\
         }\n",
    );
    assert_eq!(parsed.verdict, Verdict::Reject, "{:?}", parsed.diagnostics);
    assert!(
        parsed.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 on parse_duration result, got {:?}",
        parsed.diagnostics
    );
}

#[test]
fn real_time_duration_methods_accepted() {
    let r = check(
        "import \"time\"\n\
         import \"fmt\"\n\
         fn main() {\n\
             d := time.milliseconds(1500)\n\
             fmt.println(d.milliseconds(), d.microseconds(), d.nanoseconds(), d.seconds(), d.minutes(), d.hours(), d.abs(), d.round(time.second), d.truncate(time.millisecond), d.string())\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
