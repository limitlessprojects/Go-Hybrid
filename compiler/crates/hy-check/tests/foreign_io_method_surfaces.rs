//! Tests for curated foreign method surfaces of standard library `io` handles:
//! `io.pipe_reader`, `io.pipe_writer`, `io.section_reader`, `io.limited_reader`, and `io.offset_writer`.
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
fn phantom_pipe_reader_methods_are_rejected() {
    let bad = check(
        "import \"io\"\n\
         fn inspect(r *io.pipe_reader) {\n\
             r.totally_fake_method()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("io.pipe_reader"), "{}", diag.message);
}

#[test]
fn phantom_pipe_writer_methods_are_rejected() {
    let bad = check(
        "import \"io\"\n\
         fn inspect(w *io.pipe_writer) {\n\
             w.unknown_write_call()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("io.pipe_writer"), "{}", diag.message);
}

#[test]
fn phantom_section_reader_methods_are_rejected() {
    let bad = check(
        "import \"io\"\n\
         fn inspect(s *io.section_reader) {\n\
             s.bogus_seek_op()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("io.section_reader"), "{}", diag.message);
}

#[test]
fn phantom_limited_reader_methods_are_rejected() {
    let bad = check(
        "import \"io\"\n\
         fn inspect(l *io.limited_reader) {\n\
             l.non_existent_method()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("io.limited_reader"), "{}", diag.message);
}

#[test]
fn phantom_offset_writer_methods_are_rejected() {
    let bad = check(
        "import \"io\"\n\
         fn inspect(o *io.offset_writer) {\n\
             o.flush()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("io.offset_writer"), "{}", diag.message);
}

#[test]
fn package_level_io_functions_called_as_methods_are_rejected() {
    // io.pipe, io.copy, and io.read_all are package-level functions, NOT methods on handles
    let bad_pipe = check(
        "import \"io\"\n\
         fn inspect(r *io.pipe_reader) {\n\
             r.pipe()\n\
         }\n",
    );
    assert_eq!(bad_pipe.verdict, Verdict::Reject);
    assert!(codes(&bad_pipe).iter().any(|c| c == "HYB-E1002"));

    let bad_copy = check(
        "import \"io\"\n\
         fn inspect(w *io.pipe_writer) {\n\
             w.copy()\n\
         }\n",
    );
    assert_eq!(bad_copy.verdict, Verdict::Reject);
    assert!(codes(&bad_copy).iter().any(|c| c == "HYB-E1002"));
}

#[test]
fn real_io_methods_accept() {
    let good = check(
        "import \"io\"\n\
         fn test_pipe_reader(r *io.pipe_reader) {\n\
             r.read()\n\
             r.close()\n\
             r.close_with_error()\n\
         }\n\
         fn test_pipe_writer(w *io.pipe_writer) {\n\
             w.write()\n\
             w.close()\n\
             w.close_with_error()\n\
         }\n\
         fn test_section_reader(s *io.section_reader) {\n\
             s.read()\n\
             s.read_at()\n\
             s.seek()\n\
             s.size()\n\
             s.outer()\n\
         }\n\
         fn test_limited_reader(l *io.limited_reader) {\n\
             l.read()\n\
         }\n\
         fn test_offset_writer(o *io.offset_writer) {\n\
             o.write()\n\
             o.write_at()\n\
             o.seek()\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        good.diagnostics
    );
}
