//! Tests for curated foreign method surfaces of extended `net/http` interfaces (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on `http.flusher`, `http.hijacker`, `http.pusher`,
//! `http.file_system`, and `http.file` rejects fail-closed with HYB-E1002, while all real Go standard
//! library methods accept cleanly with 0 diagnostics.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_http_flusher_and_hijacker_rejected() {
    let bad = check(
        "struct sse {\n\
             fl http.flusher\n\
             hj http.hijacker\n\
         }\n\
         fn main() {\n\
             var s sse\n\
             s.fl.totally_fake_method()\n\
             s.hj.phantom_hijack_call()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected multiple HYB-E1002 errors, got {:?}",
        all_codes
    );
}

#[test]
fn phantom_http_pusher_rejected() {
    let bad = check(
        "fn check_pusher(p http.pusher) {\n\
             p.invalid_push_stream()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn phantom_http_filesystem_and_file_rejected() {
    let bad = check(
        "struct server {\n\
             fs http.file_system\n\
             f  http.file\n\
         }\n\
         fn main() {\n\
             var s server\n\
             s.fs.fake_open_dir()\n\
             s.f.bogus_file_op()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected multiple HYB-E1002 errors, got {:?}",
        all_codes
    );
}

#[test]
fn pointer_receivers_enforce_surface() {
    let bad = check(
        "fn check_ptrs(fl *http.flusher, f *http.file) {\n\
             fl.fake_flush_op()\n\
             f.fake_truncate()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected multiple HYB-E1002 errors, got {:?}",
        all_codes
    );
}

#[test]
fn real_http_extended_methods_accept() {
    let good = check(
        "struct sse {\n\
             fl http.flusher\n\
             hj http.hijacker\n\
             pu http.pusher\n\
             fs http.file_system\n\
             f  http.file\n\
         }\n\
         fn main() {\n\
             var s sse\n\
             _ = s.fl.flush()\n\
             _ = s.hj.hijack()\n\
             _ = s.pu.push(\"/static/app.js\", nil)\n\
             _ = s.fs.open(\"index.html\")\n\
             _ = s.f.read([]byte{})\n\
             _ = s.f.seek(0, 0)\n\
             _ = s.f.readdir(0)\n\
             _ = s.f.stat()\n\
             _ = s.f.close()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
