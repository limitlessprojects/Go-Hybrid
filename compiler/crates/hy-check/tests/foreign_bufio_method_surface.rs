//! Tests for curated foreign method surfaces of `bufio.reader`, `bufio.writer`, and `bufio.scanner`.
//! Guarantees that calling phantom/unsupported methods rejects fail-closed with HYB-E1002 (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_bufio_reader_methods_are_rejected() {
    let bad = check(
        "import \"bufio\"\n\
         struct holder {\n\
             r bufio.reader\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.r.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("bufio.reader"),
        "message: {}",
        diag.message
    );
    assert!(
        diag.message.contains("totally_fake_method"),
        "message: {}",
        diag.message
    );
    let notes = diag.notes.join(" | ");
    assert!(
        notes.contains("buffered, discard, peek, read"),
        "notes must list real methods, got: {}",
        notes
    );
}

#[test]
fn pointer_bufio_reader_phantom_method_is_rejected() {
    let bad = check(
        "import \"bufio\"\n\
         fn inspect(r *bufio.reader) {\n\
             r.bogus_read()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn real_bufio_reader_methods_accept() {
    let good = check(
        "import \"bufio\"\n\
         struct holder {\n\
             r bufio.reader\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             n := h.r.buffered()\n\
             sz := h.r.size()\n\
             fmt.println(\"buf={n} sz={sz}\")\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "real methods must accept, got {:?}",
        good.diagnostics
    );
}

#[test]
fn real_pointer_bufio_reader_methods_accept() {
    let good = check(
        "import \"bufio\"\n\
         fn inspect(r *bufio.reader) {\n\
             n := r.buffered()\n\
             sz := r.size()\n\
             fmt.println(\"buf={n} sz={sz}\")\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "real methods must accept, got {:?}",
        good.diagnostics
    );
}

#[test]
fn phantom_bufio_writer_methods_are_rejected() {
    let bad = check(
        "import \"bufio\"\n\
         struct holder {\n\
             w bufio.writer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.w.phantom_call()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("bufio.writer"),
        "message: {}",
        diag.message
    );
    assert!(
        diag.message.contains("phantom_call"),
        "message: {}",
        diag.message
    );
    let notes = diag.notes.join(" | ");
    assert!(
        notes.contains("available, available_buffer, buffered, flush"),
        "notes must list real methods, got: {}",
        notes
    );
}

#[test]
fn pointer_bufio_writer_phantom_method_is_rejected() {
    let bad = check(
        "import \"bufio\"\n\
         fn inspect(w *bufio.writer) {\n\
             w.invalid_flush()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn real_bufio_writer_methods_accept() {
    let good = check(
        "import \"bufio\"\n\
         struct holder {\n\
             w bufio.writer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             av := h.w.available()\n\
             n := h.w.buffered()\n\
             sz := h.w.size()\n\
             fmt.println(\"av={av} buf={n} sz={sz}\")\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "real methods must accept, got {:?}",
        good.diagnostics
    );
}

#[test]
fn real_pointer_bufio_writer_methods_accept() {
    let good = check(
        "import \"bufio\"\n\
         fn inspect(w *bufio.writer) {\n\
             av := w.available()\n\
             n := w.buffered()\n\
             sz := w.size()\n\
             fmt.println(\"av={av} buf={n} sz={sz}\")\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "real methods must accept, got {:?}",
        good.diagnostics
    );
}

#[test]
fn phantom_bufio_scanner_methods_are_rejected() {
    let bad = check(
        "import \"bufio\"\n\
         struct holder {\n\
             s bufio.scanner\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.s.totally_fake_scanner_call()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("bufio.scanner"),
        "message: {}",
        diag.message
    );
    assert!(
        diag.message.contains("totally_fake_scanner_call"),
        "message: {}",
        diag.message
    );
    let notes = diag.notes.join(" | ");
    assert!(
        notes.contains("buffer, bytes, err, scan, split, text"),
        "notes must list real methods, got: {}",
        notes
    );
}

#[test]
fn pointer_bufio_scanner_phantom_method_is_rejected() {
    let bad = check(
        "import \"bufio\"\n\
         fn inspect(s *bufio.scanner) {\n\
             s.unknown_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn real_bufio_scanner_methods_accept() {
    let good = check(
        "import \"bufio\"\n\
         struct holder {\n\
             s bufio.scanner\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             ok := h.s.scan()\n\
             txt := h.s.text()\n\
             fmt.println(\"ok={ok} txt={txt}\")\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "real methods must accept, got {:?}",
        good.diagnostics
    );
}

#[test]
fn real_pointer_bufio_scanner_methods_accept() {
    let good = check(
        "import \"bufio\"\n\
         fn inspect(s *bufio.scanner) {\n\
             ok := s.scan()\n\
             txt := s.text()\n\
             fmt.println(\"ok={ok} txt={txt}\")\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "real methods must accept, got {:?}",
        good.diagnostics
    );
}
