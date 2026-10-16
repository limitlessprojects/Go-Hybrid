//! channel_elem_type.rs — regression for issue #146: channel element types
//! were never checked. `ch <- 7` into `chan job` (and typed receive
//! `var n int = <-ch`) passed `hyb check` with 0 diagnostics, then failed
//! `go build` with `cannot use ... as ... value in send` / assignment —
//! breaking the accepted-should-emit-buildable-Go contract.
//!
//! Related open PR #164 covers send-only under HYB-E3008 and conflicts with
//! main; this suite pins HYB-E3011 for send *and* receive.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const SEND_INT_INTO_JOB: &str = "\
struct job { cost int }

fn main() {
    ch := make(chan job, 1)
    ch <- 7
}
";

const RECV_JOB_INTO_INT: &str = "\
struct job { cost int }

fn main() {
    ch := make(chan job, 1)
    ch <- job{cost: 1}
    var n int = <-ch
}
";

const SEND_STRING_INTO_INT: &str = "\
fn main() {
    ch := make(chan int, 1)
    ch <- \"hello\"
}
";

const SEND_INT_INTO_INT_OK: &str = "\
fn main() {
    ch := make(chan int, 1)
    ch <- 7
    v := <-ch
    fmt.println(\"{v}\")
}
";

const SEND_JOB_INTO_JOB_OK: &str = "\
struct job { cost int }

fn main() {
    ch := make(chan job, 1)
    ch <- job{cost: 1}
    var j job = <-ch
    fmt.println(\"{j.cost}\")
}
";

const SELECT_SEND_MISMATCH: &str = "\
struct job { cost int }

fn main() {
    ch := make(chan job, 1)
    select {
    ch <- 7:
        fmt.println(\"x\")
    }
}
";

const ASSIGN_RECV_MISMATCH: &str = "\
struct job { cost int }

fn main() {
    ch := make(chan job, 1)
    ch <- job{cost: 1}
    var n int
    n = <-ch
}
";

const NONSENDABLE_KEEPS_E3002: &str = "\
fn main() {
    jobs := make(chan int, 1)
    p := new(int)
    jobs <- p
}
";

#[test]
fn send_int_into_job_channel_rejects_e3011() {
    let r = check(SEND_INT_INTO_JOB);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011, got {:?} diags={:?}",
        cs,
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3011" && d.message.contains("job")),
        "message should name the element type, got {:?}",
        r.diagnostics
    );
}

#[test]
fn recv_job_into_int_rejects_e3011() {
    let r = check(RECV_JOB_INTO_INT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011 on typed receive, got {:?} diags={:?}",
        cs,
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn assign_recv_job_into_int_rejects_e3011() {
    let r = check(ASSIGN_RECV_MISMATCH);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011 on assign-from-receive, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn send_string_into_int_channel_rejects_e3011() {
    let r = check(SEND_STRING_INTO_INT);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn select_send_mismatch_rejects_e3011() {
    let r = check(SELECT_SEND_MISMATCH);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011 on select-arm send, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn matching_send_and_recv_accept() {
    let r = check(SEND_INT_INTO_INT_OK);
    assert_eq!(
        error_codes(&r),
        Vec::<String>::new(),
        "matching int channel must stay clean, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);

    let r = check(SEND_JOB_INTO_JOB_OK);
    assert_eq!(
        error_codes(&r),
        Vec::<String>::new(),
        "matching job channel must stay clean, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn nonsendable_pointer_keeps_e3002_no_cascade() {
    let r = check(NONSENDABLE_KEEPS_E3002);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 for *int send, got {:?}",
        cs
    );
    assert!(
        !cs.iter().any(|c| c == "HYB-E3011"),
        "E3011 must not cascade over E3002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}


