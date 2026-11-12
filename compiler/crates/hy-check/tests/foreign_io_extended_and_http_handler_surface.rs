//! Tests for curated foreign method surfaces of extended `io` interfaces and `http.handler` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on foreign interface handles rejects fail-closed with HYB-E1002,
//! while all real Go standard library methods accept cleanly with 0 diagnostics.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_io_string_writer_rejected() {
    let bad = check(
        "struct holder {\n\
             sw io.string_writer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.sw.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn phantom_io_byte_reader_writer_scanner_rejected() {
    for (ty, fake_call) in [
        ("io.byte_reader", "br.phantom_byte_call()"),
        ("io.byte_writer", "bw.fake_flush_byte()"),
        ("io.byte_scanner", "bs.invalid_peek_byte()"),
        ("io.rune_reader", "rr.bogus_rune_call()"),
        ("io.rune_scanner", "rs.phantom_seek_rune()"),
    ] {
        let src = format!(
            "fn check_val({} {}) {{\n\
                 {}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n",
            fake_call.split('.').next().unwrap(),
            ty,
            fake_call
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {ty}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {ty}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn phantom_io_at_and_stream_interfaces_rejected() {
    for (ty, fake_call) in [
        ("io.reader_at", "ra.fake_offset_read()"),
        ("io.writer_at", "wa.invalid_write_at_call()"),
        ("io.reader_from", "rf.bogus_stream_pull()"),
        ("io.writer_to", "wt.phantom_dump_stream()"),
    ] {
        let src = format!(
            "struct holder {{\n\
                 v {}\n\
             }}\n\
             fn main() {{\n\
                 var h holder\n\
                 h.v.{}\n\
                 fmt.println(1)\n\
             }}\n",
            ty,
            fake_call.split('.').nth(1).unwrap()
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {ty}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {ty}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn phantom_http_handler_rejected() {
    let bad = check(
        "struct server {\n\
             h http.handler\n\
         }\n\
         fn main() {\n\
             var s server\n\
             s.h.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn pointer_receivers_enforce_surface() {
    let bad = check(
        "fn check_ptrs(sw *io.string_writer, h *http.handler) {\n\
             sw.fake_flush()\n\
             h.fake_listen()\n\
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
fn real_io_extended_and_http_handler_methods_accept() {
    let good = check(
        "struct holder {\n\
             sw io.string_writer\n\
             br io.byte_reader\n\
             bw io.byte_writer\n\
             bs io.byte_scanner\n\
             rr io.rune_reader\n\
             rs io.rune_scanner\n\
             ra io.reader_at\n\
             wa io.writer_at\n\
             rf io.reader_from\n\
             wt io.writer_to\n\
             h  http.handler\n\
         }\n\
         fn main() {\n\
             var s holder\n\
             _ = s.sw.write_string(\"hello\")\n\
             _ = s.br.read_byte()\n\
             _ = s.bw.write_byte(65)\n\
             _ = s.bs.read_byte()\n\
             _ = s.bs.unread_byte()\n\
             _ = s.rr.read_rune()\n\
             _ = s.rs.read_rune()\n\
             _ = s.rs.unread_rune()\n\
             _ = s.ra.read_at([]byte{}, 0)\n\
             _ = s.wa.write_at([]byte{}, 0)\n\
             _ = s.h.serve_http(nil, nil)\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
