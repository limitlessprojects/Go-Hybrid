//! time_call_arity_and_operand_bypass.rs: regression tests for `time.now`,
//! `time.sleep`, `time.since`, `time.until`, `time.after`, and `time.tick` member calls.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! Go code that compiles under `go build`.
//!
//! Standard Go §time:
//! - `func Now() Time` (0 arguments)
//! - `func Sleep(d Duration)` (1 argument)
//! - `func Since(t Time) Duration` (1 argument)
//! - `func Until(t Time) Duration` (1 argument)
//! - `func After(d Duration) <-chan Time` (1 argument)
//! - `func Tick(d Duration) <-chan Time` (1 argument)
//!
//! Calls with missing or extra arguments (e.g. `time.now(1)`, `time.sleep()`,
//! `time.since()`, `time.until()`, `time.after()`, `time.tick()`) or invalid
//! literal operands must be rejected with `HYB-E1002`.

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

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for {:?}, got {:?}",
        src,
        r.diagnostics
    );
}

#[test]
fn rejects_time_now_with_arguments() {
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.now(1)\n\
         }",
    );
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.now(\"extra\")\n\
         }",
    );
}

#[test]
fn rejects_time_sleep_invalid_arity() {
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             time.sleep()\n\
         }",
    );
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             time.sleep(1, 2)\n\
         }",
    );
}

#[test]
fn rejects_time_sleep_invalid_operand() {
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             time.sleep(\"1s\")\n\
         }",
    );
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             time.sleep(nil)\n\
         }",
    );
}

#[test]
fn rejects_time_since_invalid_arity_or_operand() {
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.since()\n\
         }",
    );
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.since(123)\n\
         }",
    );
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.since(\"now\")\n\
         }",
    );
}

#[test]
fn rejects_time_until_invalid_arity_or_operand() {
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.until()\n\
         }",
    );
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.until(nil)\n\
         }",
    );
}

#[test]
fn rejects_time_after_tick_zero_args() {
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.after()\n\
         }",
    );
    rejects(
        "import \"time\"\n\
         fn main() {\n\
             _ = time.tick()\n\
         }",
    );
}

#[test]
fn accepts_valid_time_calls() {
    accepts(
        "import \"time\"\n\
         fn main() {\n\
             t := time.now()\n\
             time.sleep(time.seconds(1))\n\
             d := time.since(t)\n\
             u := time.until(t)\n\
             _ = d\n\
             _ = u\n\
         }",
    );
}
