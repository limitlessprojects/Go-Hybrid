//! Regression tests for #4076:
//! channel direction checks on built-in `close()` and `Stmt::Send` must
//! see through curated foreign calls (`time.after`) and struct method
//! calls (`r.get_rx()`) that return directional channels.
//!
//! Before the fix, all four bypass forms below fall through to
//! `dir_ty_of_expr -> None`, so `chan_dir_of_expr_env` returns `None` and
//! both `emit_close_recv_only` (HYB-E3009) and `emit_send_recv_only`
//! (HYB-E3008) fail open — `hyb check` Accepts programs whose emitted Go
//! cannot compile (GUARANTEES.md row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn error_codes(src: &str) -> Vec<String> {
    let r = check(src);
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(code: &str, src: &str) {
    let cs = error_codes(src);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {code}, got {cs:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let cs = error_codes(src);
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---- the four bypass forms from the issue ----

#[test]
fn close_on_foreign_receive_only_channel_rejects_e3009() {
    assert_has(
        "HYB-E3009",
        "import \"time\"\nfn main() {\n    close(time.after(10))\n}\n",
    );
}

#[test]
fn send_to_foreign_receive_only_channel_rejects_e3008() {
    assert_has(
        "HYB-E3008",
        "import \"time\"\nfn main() {\n    time.after(10) <- time.now()\n}\n",
    );
}

#[test]
fn close_on_method_returned_receive_only_channel_rejects_e3009() {
    assert_has(
        "HYB-E3009",
        "struct receiver {\n    rx <-chan int\n}\nfn (r receiver) get_rx() <-chan int {\n    return r.rx\n}\nfn main() {\n    var r receiver\n    close(r.get_rx())\n}\n",
    );
}

#[test]
fn send_to_method_returned_receive_only_channel_rejects_e3008() {
    assert_has(
        "HYB-E3008",
        "struct receiver {\n    rx <-chan int\n}\nfn (r receiver) get_rx() <-chan int {\n    return r.rx\n}\nfn main() {\n    var r receiver\n    r.get_rx() <- 42\n}\n",
    );
}

// ---- pointer receiver variant (issue: also Accepts) ----

#[test]
fn close_on_pointer_method_returned_receive_only_channel_rejects_e3009() {
    assert_has(
        "HYB-E3009",
        "struct receiver {\n    rx <-chan int\n}\nfn (r *receiver) get_rx() <-chan int {\n    return r.rx\n}\nfn consume(r *receiver) {\n    close(r.get_rx())\n}\nfn main() {}\n",
    );
}

// ---- control from the issue: direct identifier already rejects ----

#[test]
fn direct_identifier_controls_keep_rejecting() {
    assert_has(
        "HYB-E3009",
        "fn control(rx <-chan int) { close(rx) }\nfn main() {}\n",
    );
    assert_has(
        "HYB-E3008",
        "fn control(rx <-chan int) { rx <- 1 }\nfn main() {}\n",
    );
}

// ---- positive controls: the fix must not over-reject ----

#[test]
fn bidirectional_channel_through_method_still_accepts() {
    assert_accept(
        "struct relay {\n    ch chan int\n}\nfn (r relay) get_ch() chan int {\n    return r.ch\n}\nfn consume(r relay) {\n    r.get_ch() <- 1\n    close(r.get_ch())\n}\nfn main() {}\n",
    );
}

#[test]
fn foreign_send_capable_call_still_accepts() {
    // make(chan int) is bidirectional: send and close must stay legal.
    assert_accept("fn main() {\n    ch := make(chan int)\n    ch <- 1\n    close(ch)\n}\n");
}
