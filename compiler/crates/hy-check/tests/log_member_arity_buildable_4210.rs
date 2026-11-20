//! Regression tests for #4210 (companion to #4135 / #4238 package call-arity family):
//! The full `log` call surface must be arity-checked at `hyb check` time against
//! Go 1.26 standard library signatures (`go doc log`), and non-callable package
//! flag constants and types (`ldate`, `ltime`, `lmicroseconds`, `llongfile`,
//! `lshortfile`, `lutc`, `lmsgprefix`, `lstdflags`, `logger`) must reject as call
//! targets with `HYB-E1002` (Row 13 BUILDABLE).
//!
//! Crucially, pure-variadic `log.print`, `log.println`, `log.fatal`, `log.fatalln`,
//! `log.panic`, and `log.panicln` have Go signature `func(v ...any)` and MUST accept
//! 0 arguments as well as 1+ arguments (never over-reject), whereas format-variadic
//! `log.printf`, `log.fatalf`, and `log.panicf` have Go signature
//! `func(format string, v ...any)` and require at least 1 argument.

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

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "verdict for {:?}: {:?}",
        src,
        r.diagnostics
    );
}

#[test]
fn log_zero_arg_members_accept_exact_arity() {
    accepts("import \"log\"\nfn main() {\n    f := log.flags()\n    _ = f\n}\n");
    accepts("import \"log\"\nfn main() {\n    p := log.prefix()\n    _ = p\n}\n");
    accepts("import \"log\"\nfn main() {\n    w := log.writer()\n    _ = w\n}\n");
}

#[test]
fn log_one_two_three_arg_members_accept_exact_arity() {
    accepts("import \"log\"\nfn main() {\n    log.set_flags(log.lstdflags)\n}\n");
    accepts("import \"log\"\nfn main() {\n    log.set_prefix(\"app: \")\n}\n");
    accepts("import \"log\"\nfn main() {\n    w := log.writer()\n    log.set_output(w)\n}\n");
    accepts("import \"log\"\nfn main() {\n    err := log.output(1, \"msg\")\n    _ = err\n}\n");
    accepts("import \"log\"\nfn main() {\n    w := log.writer()\n    l := log.new(w, \"app: \", log.lstdflags)\n    _ = l\n}\n");
}

#[test]
fn log_pure_variadic_members_accept_zero_and_multiple_args() {
    // In Go (`go doc log`), Print/Println/Fatal/Fatalln/Panic/Panicln have
    // signature `func(v ...any)` — 0 arguments is legal Go and must never be
    // false-rejected.
    accepts("import \"log\"\nfn main() {\n    log.print()\n    log.println()\n    log.print(\"a\", 1)\n    log.println(\"a\", 1)\n}\n");
    accepts("import \"log\"\nfn h1() {\n    log.fatal()\n}\nfn h2() {\n    log.fatalln()\n}\nfn main() {}\n");
    accepts("import \"log\"\nfn h1() {\n    log.panic()\n}\nfn h2() {\n    log.panicln()\n}\nfn main() {}\n");
    accepts("import \"log\"\nfn main() {\n    log.fatal(\"err\", 1)\n    log.fatalln(\"err\", 1)\n    log.panic(\"err\", 1)\n    log.panicln(\"err\", 1)\n}\n");
}

#[test]
fn log_format_variadic_members_accept_one_or_more_and_reject_zero() {
    accepts("import \"log\"\nfn main() {\n    log.printf(\"hello\")\n    log.printf(\"n=%d\", 42)\n    log.fatalf(\"err=%s\", \"x\")\n    log.panicf(\"err=%s\", \"x\")\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.printf()\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.fatalf()\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.panicf()\n}\n");
}

#[test]
fn log_fixed_arity_members_reject_wrong_arity() {
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.flags(1)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.prefix(\"x\")\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.writer(nil)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.set_flags()\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.set_flags(1, 2)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.set_prefix()\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.set_prefix(\"a\", \"b\")\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.set_output()\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    log.set_output(nil, nil)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.output()\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.output(1)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.output(1, \"msg\", 3)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.new()\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.new(nil)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.new(nil, \"p\")\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    _ = log.new(nil, \"p\", 0, 1)\n}\n");
}

#[test]
fn log_package_constants_and_types_are_not_callable() {
    for member in [
        "ldate",
        "ltime",
        "lmicroseconds",
        "llongfile",
        "lshortfile",
        "lutc",
        "lmsgprefix",
        "lstdflags",
        "logger",
    ] {
        rejects_e1002(&format!(
            "import \"log\"\nfn main() {{\n    _ = log.{member}()\n}}\n"
        ));
        rejects_e1002(&format!(
            "import \"log\"\nfn main() {{\n    _ = log.{member}(1)\n}}\n"
        ));
    }

    // Value-position reference to all 8 flag constants still accepts cleanly.
    accepts(
        "import \"log\"\n\
         fn main() {\n\
             f := log.ldate | log.ltime | log.lmicroseconds | log.llongfile | log.lshortfile | log.lutc | log.lmsgprefix | log.lstdflags\n\
             log.set_flags(f)\n\
         }\n",
    );
}

#[test]
fn log_spread_is_rejected_on_non_variadic_calls() {
    rejects_e1002("import \"log\"\nfn main() {\n    xs := []int{1}\n    log.set_flags(xs...)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    xs := []string{\"a\", \"b\"}\n    _ = log.output(xs...)\n}\n");
    rejects_e1002("import \"log\"\nfn main() {\n    xs := []int{1, 2, 3}\n    _ = log.new(xs...)\n}\n");
}

#[test]
fn log_arity_error_message_is_actionable() {
    let r = check("import \"log\"\nfn main() {\n    log.set_flags(1, 2)\n}\n");
    let msg = format!("{:?}", r.diagnostics);
    assert!(
        msg.contains("takes exactly 1 argument"),
        "message must state expected arity: {}",
        msg
    );
    assert!(
        msg.contains("got 2"),
        "message must state actual count: {}",
        msg
    );
    assert!(
        msg.contains("SetFlags"),
        "message must name the Go lowering: {}",
        msg
    );
}
