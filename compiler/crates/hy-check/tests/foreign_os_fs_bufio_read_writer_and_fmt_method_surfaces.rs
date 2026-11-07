//! Regression tests for curated foreign method surfaces and result shapes on
//! `os.file_info`, `os.file_mode`, `os.dir_entry`, `os.signal`,
//! `bufio.read_writer`, `fmt.stringer`, `fmt.go_stringer`, `fmt.state`,
//! `fmt.formatter`, `fmt.scanner`, `fmt.scan_state`, `sync.locker`,
//! `sort.interface`, `time.month`, and `time.weekday`
//! (`HYB-E1002`, GUARANTEES.md Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that compiles under `go build`. Calling a phantom or
//! ambiguous method on any of these curated standard library handles previously
//! passed `hyb check` with 0 diagnostics and died in `go build` with
//! `type ... has no field or method ...` or `ambiguous selector ...`.

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
        "must accept with 0 errors, got: {:?} ({:?}) for\n{}",
        codes,
        r.diagnostics,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn test_os_file_info_phantom_method_rejects() {
    let src = r#"
import "os"

struct state {
    info os.file_info
}

fn main() {
    var s state
    s.info.totally_fake()
}
"#;
    rejects_e1002(src, "os.file_info");
}

#[test]
fn test_os_file_mode_phantom_method_rejects() {
    let src = r#"
import "os"

struct state {
    mode os.file_mode
}

fn main() {
    var s state
    s.mode.chmod_bits()
}
"#;
    rejects_e1002(src, "os.file_mode");
}

#[test]
fn test_os_dir_entry_phantom_method_rejects() {
    let src = r#"
import "os"

struct state {
    entry os.dir_entry
}

fn main() {
    var s state
    s.entry.mod_time()
}
"#;
    rejects_e1002(src, "os.dir_entry");
}

#[test]
fn test_os_signal_phantom_method_rejects() {
    let src = r#"
import "os"

struct state {
    sig os.signal
}

fn main() {
    var s state
    s.sig.kill_now()
}
"#;
    rejects_e1002(src, "os.signal");
}

#[test]
fn test_bufio_read_writer_phantom_and_ambiguous_methods_reject() {
    // 1. Arbitrary phantom method must reject.
    let src_phantom = r#"
import "bufio"

fn inspect(rw *bufio.read_writer) {
    rw.totally_fake()
}
"#;
    rejects_e1002(src_phantom, "bufio.read_writer");

    // 2. In Go's `bufio.ReadWriter` (`struct { *Reader; *Writer }`), `Buffered`,
    // `Reset`, and `Size` exist on BOTH embedded structs at the same depth and
    // are excluded from `*bufio.ReadWriter`'s method set (`ambiguous selector`).
    for ambiguous in ["buffered()", "reset()", "size()"] {
        let src = format!(
            "import \"bufio\"\n\
             fn inspect(rw *bufio.read_writer) {{\n\
                 rw.{ambiguous}\n\
             }}\n"
        );
        rejects_e1002(&src, "bufio.read_writer");
    }
}

#[test]
fn test_fmt_interfaces_phantom_methods_reject() {
    for (ty, call) in [
        ("fmt.stringer", "go_string()"),
        ("fmt.go_stringer", "string()"),
        ("fmt.state", "scan()"),
        ("fmt.formatter", "write()"),
        ("fmt.scanner", "format()"),
        ("fmt.scan_state", "flush()"),
    ] {
        let src = format!(
            "import \"fmt\"\n\
             fn inspect(h {ty}) {{\n\
                 h.{call}\n\
             }}\n"
        );
        rejects_e1002(&src, ty);
    }
}

#[test]
fn test_sync_locker_and_time_calendar_phantom_methods_reject() {
    for (pkg, ty, call) in [
        ("sync", "sync.locker", "try_lock()"),
        ("time", "time.month", "hours()"),
        ("time", "time.weekday", "minutes()"),
    ] {
        let src = format!(
            "import \"{pkg}\"\n\
             fn inspect(h {ty}) {{\n\
                 h.{call}\n\
             }}\n"
        );
        rejects_e1002(&src, ty);
    }
}

#[test]
fn test_method_result_chaining_gates_returned_file_mode_handles() {
    // `de.info()` returns `os.file_info`, `fi.mode()` returns `os.file_mode`, and
    // `fm.perm()` returns `os.file_mode`, so calling a phantom method on any of the
    // bound results must also reject with HYB-E1002.
    let src_from_info = r#"
import "os"

fn inspect(fi os.file_info) {
    m := fi.mode()
    m.totally_fake()
}
"#;
    rejects_e1002(src_from_info, "os.file_mode");

    let src_from_dir_entry = r#"
import "os"

fn inspect(de os.dir_entry) {
    fi := de.info()
    m := fi.mode()
    p := m.perm()
    p.bogus_bit_op()
}
"#;
    rejects_e1002(src_from_dir_entry, "os.file_mode");
}

#[test]
fn test_real_methods_and_result_shapes_accept() {
    let src = r#"
import "bufio"
import "fmt"
import "os"
import "sync"
import "time"

fn worker(name string, sz i64, is_reg bool) {
    fmt.println(name, sz, is_reg)
}

fn inspect(
    fi os.file_info,
    de os.dir_entry,
    sig os.signal,
    rw *bufio.read_writer,
    s fmt.stringer,
    gs fmt.go_stringer,
    st fmt.state,
    f fmt.formatter,
    sc fmt.scanner,
    ss fmt.scan_state,
    lk sync.locker,
    mo time.month,
    wd time.weekday,
) {
    fi2 := de.info()
    m := fi.mode()
    p := m.perm()
    m2 := fi2.mode()
    is_reg := p.is_regular() && !m2.is_dir() && !de.is_dir()
    name := fi.name()
    sz := fi.size()
    _ = de.name()
    _ = sig.string()
    _ = s.string()
    _ = gs.go_string()
    _ = st.flag(43)
    _ = rw.available()
    _ = rw.flush()
    _ = ss.unread_rune()
    lk.lock()
    lk.unlock()
    _ = mo.string()
    _ = wd.string()
    _ = f
    _ = sc
    task worker(name, sz, is_reg)
}

fn main() {
    fmt.println("ok")
}
"#;
    accepts(src);
}

