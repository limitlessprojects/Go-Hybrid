//! Regression tests for rust/sha3 call arity validation under Row 13 (BUILDABLE):
//! The full `rust/sha3` call surface must be arity-checked at `hyb check`
//! time. Before this, calls to `sha3.digest256` and `sha3.stream_checksum`
//! escaped with arbitrary arity with 0 diagnostics, failing at emitter/build
//! time instead with `missing arguments` or `too many arguments` (Row 13
//! BUILDABLE: reject fail-closed, HYB-E1002).
//!
//! Arities mirror the pinned Rust crate descriptors in hy-emit/src/shapes.rs:
//!   * digest256(data): exactly 1 argument
//!   * stream_checksum(iters, seed): exactly 2 arguments
//! Spread calls (`...`) on non-variadic members are strictly guarded.

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
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn sha3_exact_arity_calls_accept() {
    accepts("import \"rust/sha3\"\nfn main() {\n    h := sha3.digest256([]byte(\"hello\"))\n    _ = h\n}\n");
    accepts("import \"sha3\"\nfn main() {\n    h := sha3.digest256([]byte(\"world\"))\n    _ = h\n}\n");
    accepts("import \"rust/sha3\"\nfn main() {\n    c := sha3.stream_checksum(100, 42)\n    _ = c\n}\n");
    accepts("import \"sha3\"\nfn main() {\n    c := sha3.stream_checksum(50, 7)\n    _ = c\n}\n");
}

#[test]
fn sha3_digest256_rejects_wrong_arity() {
    rejects_e1002("import \"rust/sha3\"\nfn main() {\n    _ = sha3.digest256()\n}\n");
    rejects_e1002("import \"rust/sha3\"\nfn main() {\n    _ = sha3.digest256([]byte(\"a\"), []byte(\"b\"))\n}\n");
}

#[test]
fn sha3_stream_checksum_rejects_wrong_arity() {
    rejects_e1002("import \"rust/sha3\"\nfn main() {\n    _ = sha3.stream_checksum()\n}\n");
    rejects_e1002("import \"rust/sha3\"\nfn main() {\n    _ = sha3.stream_checksum(100)\n}\n");
    rejects_e1002("import \"rust/sha3\"\nfn main() {\n    _ = sha3.stream_checksum(100, 42, 99)\n}\n");
}

#[test]
fn sha3_spread_is_rejected_on_non_variadic_calls() {
    rejects_e1002("import \"rust/sha3\"\nfn main() {\n    xs := [\"hi\"]\n    _ = sha3.digest256(xs...)\n}\n");
    rejects_e1002("import \"rust/sha3\"\nfn main() {\n    args := [100, 42]\n    _ = sha3.stream_checksum(args...)\n}\n");
}

#[test]
fn sha3_local_shadowing_and_unrelated_calls_still_work() {
    accepts("fn main() {\n    sha3 := fn(x int) int { return x * 2 }\n    v := sha3(10)\n    _ = v\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    s := strconv.itoa(42)\n    _ = s\n}\n");
}

#[test]
fn sha3_arity_error_message_is_actionable() {
    let r = check("import \"rust/sha3\"\nfn main() {\n    _ = sha3.digest256()\n}\n");
    let msg = format!("{:?}", r.diagnostics);
    assert!(msg.contains("takes exactly 1 argument"), "message must state expected arity: {}", msg);
    assert!(msg.contains("got 0"), "message must state actual count: {}", msg);
}
