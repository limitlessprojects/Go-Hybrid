//! Regression tests for the curated foreign method surface of `log.Logger`
//! (`log.logger` / `log.Logger`, `HYB-E1002`, GUARANTEES.md Row 13 BUILDABLE).
//!
//! Guarantees that invoking phantom/unsupported methods or package-only functions on
//! `log.logger`, `*log.logger`, struct fields, or `log.default()` / `log.new(...)`
//! bindings rejects fail-closed with `HYB-E1002`, while all 16 standard Go methods
//! on `*log.Logger` accept cleanly with 0 diagnostics.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str, expected_substring: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "must emit HYB-E1002, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains(expected_substring)),
        "diagnostic message must contain {:?}, got: {:?}",
        expected_substring,
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

fn accepts(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "must accept with 0 errors, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn phantom_log_logger_value_and_pointer_rejected() {
    rejects_e1002(
        "import \"log\"\n\
         fn handle(l log.logger) {\n\
             l.totally_fake_log_method()\n\
         }\n\
         fn main() {}\n",
        "log.logger",
    );

    rejects_e1002(
        "import \"log\"\n\
         fn handle(l *log.logger) {\n\
             l.bogus_logger_call()\n\
         }\n\
         fn main() {}\n",
        "log.logger",
    );
}

#[test]
fn phantom_log_logger_struct_field_and_package_constructors_rejected() {
    rejects_e1002(
        "import \"log\"\n\
         struct service {\n\
             lg log.logger\n\
         }\n\
         fn main() {\n\
             var s service\n\
             s.lg.non_existent_method()\n\
         }\n",
        "log.logger",
    );

    // `new` and `default` are package-level functions in `log`, not methods on `*log.Logger`.
    rejects_e1002(
        "import \"log\"\n\
         fn handle(l *log.logger) {\n\
             l.default()\n\
         }\n\
         fn main() {}\n",
        "log.logger",
    );

    rejects_e1002(
        "import \"log\"\n\
         fn main() {\n\
             l := log.default()\n\
             l.unknown_logger_op()\n\
         }\n",
        "log.logger",
    );

    rejects_e1002(
        "import \"log\"\n\
         fn main() {\n\
             l := log.new(os.stdout, \"app: \", log.lstdflags)\n\
             l.new()\n\
         }\n",
        "log.logger",
    );
}

#[test]
fn real_log_logger_methods_and_result_shapes_accept() {
    accepts(
        "import \"log\"\n\
         fn inspect(l *log.logger) {\n\
             var f int = l.flags()\n\
             var p string = l.prefix()\n\
             _ = f\n\
             _ = p\n\
             _ = l.writer()\n\
             _ = l.output(1, \"msg\")\n\
             l.set_flags(log.lstdflags)\n\
             l.set_prefix(\"p: \")\n\
             l.set_output(os.stderr)\n\
             l.print(\"a\")\n\
             l.println(\"b\")\n\
             l.printf(\"%s\", \"c\")\n\
             l.fatal(\"d\")\n\
             l.fatalln(\"e\")\n\
             l.fatalf(\"%s\", \"f\")\n\
             l.panic(\"g\")\n\
             l.panicln(\"h\")\n\
             l.panicf(\"%s\", \"i\")\n\
         }\n\
         fn main() {\n\
             d := log.default()\n\
             d.println(\"ok\")\n\
         }\n",
    );
}
