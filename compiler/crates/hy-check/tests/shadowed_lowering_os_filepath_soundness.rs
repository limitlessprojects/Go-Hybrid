//! shadowed_lowering_os_filepath_soundness.rs: regression tests for shadowed lowering packages.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! buildable Go code.
//!
//! 1. `fmt.print` on byte slices (`[]byte`) lowers to `os.Stdout.Write` in `hy-emit`.
//!    When an in-scope parameter, local binding, or package declaration named `os` exists,
//!    emitted Go resolves `os.Stdout` to the shadowing entity, causing downstream `go build`
//!    to fail with `os.Stdout undefined`.
//!
//! 2. `fn main()` with an error/exit return type or `?` operator lowers into an outer wrapper
//!    closure calling `fmt.Fprintln(os.Stderr, filepath.Base(os.Args[0])+":", hyErr)` and `os.Exit(1)`.
//!    When `filepath`, `os`, or `fmt` is shadowed by a package-level declaration, emitted Go fails
//!    downstream compilation because the imported packages are masked.
//!
//! `hy-check` must reject these shadowed lowering invocations with `HYB-E1002`.

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
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_main_error_with_package_const_filepath_rejects() {
    let src = r#"
import "errors"

const filepath = "custom/path"

fn main() error {
    return errors.new("boom")
}
"#;
    rejects(src);
}

#[test]
fn test_main_error_with_package_struct_filepath_rejects() {
    let src = r#"
import "errors"

struct filepath {
    name string
}

fn main() error {
    return errors.new("boom")
}
"#;
    rejects(src);
}

#[test]
fn test_main_error_with_package_const_os_rejects() {
    let src = r#"
import "errors"

const os = "darwin"

fn main() error {
    return errors.new("boom")
}
"#;
    rejects(src);
}

#[test]
fn test_main_error_with_package_fn_os_rejects() {
    let src = r#"
import "errors"

fn os() {}

fn main() error {
    return errors.new("boom")
}
"#;
    rejects(src);
}

#[test]
fn test_main_error_with_package_const_fmt_rejects() {
    let src = r#"
import "errors"

const fmt = 42

fn main() error {
    return errors.new("boom")
}
"#;
    rejects(src);
}

#[test]
fn test_main_try_with_package_const_filepath_rejects() {
    let src = r#"
import "errors"

const filepath = "app/dir"

fn step() error {
    return errors.new("fail")
}

fn main() {
    step()?
}
"#;
    rejects(src);
}

#[test]
fn test_main_multi_return_with_package_const_os_rejects() {
    let src = r#"
import "errors"

const os = "linux"

fn main() (int, error) {
    return 1, errors.new("fail")
}
"#;
    rejects(src);
}

#[test]
fn test_fmt_print_byte_slice_with_local_os_rejects() {
    let src = r#"
import "fmt"

fn main() {
    var b []byte = make([]byte, 4)
    os := "local"
    fmt.print(b)
}
"#;
    rejects(src);
}

#[test]
fn test_fmt_print_byte_slice_with_param_os_rejects() {
    let src = r#"
import "fmt"

fn dump(b []byte, os string) {
    fmt.print(b)
}

fn main() {}
"#;
    rejects(src);
}

#[test]
fn test_fmt_print_byte_slice_with_package_const_os_rejects() {
    let src = r#"
import "fmt"

const os = "windows"

fn main() {
    var b []byte = make([]byte, 4)
    fmt.print(b)
}
"#;
    rejects(src);
}

#[test]
fn test_fmt_print_io_read_all_with_local_os_rejects() {
    let src = r#"
import "fmt"
import "io"

fn inspect(r io.reader) {
    os := 1
    data := io.read_all(r)?
    fmt.print(data)
}

fn main() {}
"#;
    rejects(src);
}

#[test]
fn test_positive_control_main_error_unshadowed_accepts() {
    let src = r#"
import "errors"

fn main() error {
    return errors.new("ok")
}
"#;
    accepts(src);
}

#[test]
fn test_positive_control_fmt_print_bytes_unshadowed_accepts() {
    let src = r#"
import "fmt"

fn main() {
    var b []byte = make([]byte, 4)
    fmt.print(b)
}
"#;
    accepts(src);
}

#[test]
fn test_positive_control_fmt_print_string_with_os_accepts() {
    let src = r#"
import "fmt"

fn main() {
    os := "linux"
    fmt.print("hello", os)
}
"#;
    accepts(src);
}

#[test]
fn test_positive_control_non_main_fn_error_with_filepath_accepts() {
    let src = r#"
import "errors"

const filepath = "path/to/log"

fn helper() error {
    return errors.new("custom helper error")
}

fn main() {}
"#;
    accepts(src);
}
