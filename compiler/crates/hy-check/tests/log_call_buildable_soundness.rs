//! log_call_buildable_soundness.rs — regression tests for log member call arity and non-callable symbols
//! (HYB-E1002, Row 13 BUILDABLE soundness).
//!
//! Go standard library contract:
//! - `log.Flags()`, `log.Prefix()`, `log.Writer()` take exactly 0 arguments.
//! - `log.SetFlags(flag)`, `log.SetPrefix(prefix)`, `log.SetOutput(w)` take exactly 1 argument.
//! - `log.Output(calldepth, s)` takes exactly 2 arguments.
//! - `log.Print(...)`, `log.Println(...)`, `log.Fatal(...)`, `log.Fatalln(...)`,
//!   `log.Panic(...)`, `log.Panicln(...)` require at least 1 argument.
//! - `log.Printf(...)`, `log.Fatalf(...)`, `log.Panicf(...)` require at least 1 argument (format string).
//! - Non-callable flag constants (`log.Ldate`, `log.Ltime`, `log.Lmicroseconds`,
//!   `log.Llongfile`, `log.Lshortfile`, `log.LUTC`, `log.Lmsgprefix`, `log.LstdFlags`)
//!   and struct types must be rejected when invoked as functions.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
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
        "expected HYB-E1002 for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected zero errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

#[test]
fn test_log_zero_arg_functions_arity_reject() {
    // log.flags takes 0 arguments
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.flags(1)
}
"#,
    );
    // log.prefix takes 0 arguments
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.prefix("err")
}
"#,
    );
    // log.writer takes 0 arguments
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.writer(1)
}
"#,
    );
}

#[test]
fn test_log_one_arg_functions_arity_reject() {
    // log.set_flags takes 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.set_flags()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.set_flags(1, 2)
}
"#,
    );

    // log.set_prefix takes 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.set_prefix()
}
"#,
    );

    // log.set_output takes 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.set_output()
}
"#,
    );
}

#[test]
fn test_log_two_arg_functions_arity_reject() {
    // log.output takes 2 arguments
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.output()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.output(1)
}
"#,
    );
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.output(1, "msg", 3)
}
"#,
    );
}

#[test]
fn test_log_variadic_functions_arity_reject() {
    // log.print requires at least 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.print()
}
"#,
    );

    // log.println requires at least 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.println()
}
"#,
    );

    // log.printf requires at least 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.printf()
}
"#,
    );

    // log.fatal requires at least 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.fatal()
}
"#,
    );

    // log.panic requires at least 1 argument
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    log.panic()
}
"#,
    );
}

#[test]
fn test_log_non_callable_symbols_reject() {
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.lstdflags()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.ldate()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.ltime()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.lmicroseconds()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "log"

fn main() {
    _ = log.logger()
}
"#,
    );
}

#[test]
fn test_log_positive_controls_accept() {
    // When log is a shadowed user variable/struct, calling methods keeps user semantics
    accepts(
        r#"
struct my_log {
    val int
}

fn (l my_log) flags() int {
    return l.val
}

fn main() {
    log := my_log{val: 10}
    _ = log.flags()
}
"#,
    );
}
