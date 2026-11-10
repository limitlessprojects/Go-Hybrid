//! Tests for curated foreign method surfaces of `http.response_writer` and `http.round_tripper` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invoking phantom/unsupported methods on `http.response_writer` and `http.round_tripper`
//! rejects fail-closed with HYB-E1002, while standard Go methods (`header`, `write`, `write_header`, `round_trip`)
//! accept cleanly with 0 diagnostics.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_http_response_writer_rejected() {
    let bad = check(
        "fn handle(w http.response_writer) {\n\
             w.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        all_codes
    );
    let msg = &bad.diagnostics[0].message;
    assert!(msg.contains("totally_fake_method"), "message: {msg}");
    assert!(msg.contains("http.response_writer"), "message: {msg}");
}

#[test]
fn phantom_http_round_tripper_rejected() {
    let bad = check(
        "struct client {\n\
             rt http.round_tripper\n\
         }\n\
         fn main() {\n\
             var c client\n\
             c.rt.invalid_round_trip_call()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        all_codes
    );
    let msg = &bad.diagnostics[0].message;
    assert!(msg.contains("invalid_round_trip_call"), "message: {msg}");
    assert!(msg.contains("http.round_tripper"), "message: {msg}");
}

#[test]
fn pointer_receivers_enforce_surface() {
    let bad = check(
        "fn check_receivers(w *http.response_writer, rt *http.round_tripper) {\n\
             w.bogus_flush()\n\
             rt.bogus_cancel()\n\
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
fn real_http_response_writer_and_round_tripper_methods_accept() {
    let good = check(
        "fn handle(w http.response_writer, rt http.round_tripper, req http.request) {\n\
             _ = w.header()\n\
             _ = w.write([]byte{})\n\
             w.write_header(200)\n\
             _ = rt.round_trip(&req)\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
