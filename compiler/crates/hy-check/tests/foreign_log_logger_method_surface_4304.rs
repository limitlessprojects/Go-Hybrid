//! Regression (#4304): `log.logger` / `log.Logger` must expose Go's curated
//! 16-method `*log.Logger` surface in `foreign_method_surface`, infer
//! `log.new(...)` / `log.default()` in `package_result`, and model return
//! shapes in `method_result` so phantom methods reject fail-closed with
//! `HYB-E1002` (GUARANTEES.md row 13 BUILDABLE).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_and_package_only_log_logger_methods_are_rejected() {
    let cases = [
        "import \"log\"\n\
         fn main() {\n\
             l := log.default()\n\
             l.totally_fake_log_method()\n\
         }\n",
        "import \"log\"\n\
         import \"os\"\n\
         fn main() {\n\
             l := log.new(os.stdout, \"app: \", 0)\n\
             l.default()\n\
         }\n",
        "import \"log\"\n\
         fn check_ptr(l *log.logger) {\n\
             l.new()\n\
         }\n\
         fn main() {}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "phantom log.Logger call must reject, got {:?}",
            r.diagnostics
        );
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn curated_log_logger_surface_and_result_shapes_stay_accepted() {
    let src = "import \"log\"\n\
        import \"os\"\n\
        import \"fmt\"\n\
        fn main() {\n\
            l := log.default()\n\
            l2 := log.new(os.stdout, \"pref: \", 0)\n\
            l.set_flags(1)\n\
            l.set_prefix(\"p: \")\n\
            l.set_output(os.stderr)\n\
            f := l.flags()\n\
            p := l.prefix()\n\
            w := l.writer()\n\
            err := l.output(2, \"msg\")\n\
            l.print(\"a\")\n\
            l.printf(\"%s %d\", p, f + 1)\n\
            l.println(\"b\")\n\
            l2.fatal(\"f\")\n\
            l2.fatalf(\"%s\", p)\n\
            l2.fatalln(\"fl\")\n\
            l2.panic(\"p\")\n\
            l2.panicf(\"%v\", w)\n\
            l2.panicln(err.error())\n\
            fmt.println(\"{p} {f}\")\n\
        }\n";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "curated log.Logger surface must accept, got {:?}",
        r.diagnostics
    );
}
