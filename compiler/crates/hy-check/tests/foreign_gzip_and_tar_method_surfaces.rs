//! Regression tests for curated foreign method surfaces of `compress/gzip`
//! (`gzip.reader`, `gzip.writer`) and `archive/tar` (`tar.reader`, `tar.writer`,
//! `tar.header`) handles (`HYB-E1002`, GUARANTEES.md Row 13 BUILDABLE).

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
        "must accept with 0 errors, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn phantom_gzip_reader_and_writer_methods_rejected() {
    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn inspect(r *gzip.reader) {\n\
             r.flush()\n\
         }\n\
         fn main() {}\n",
        "gzip.reader",
    );

    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn inspect(w *gzip.writer) {\n\
             w.multistream(true)\n\
         }\n\
         fn main() {}\n",
        "gzip.writer",
    );

    rejects_e1002(
        "import \"compress/gzip\"\n\
         fn main() {\n\
             w := gzip.new_writer(os.stdout)\n\
             w.totally_fake_gzip_call()\n\
         }\n",
        "gzip.writer",
    );
}

#[test]
fn phantom_tar_reader_writer_and_header_methods_rejected() {
    rejects_e1002(
        "import \"archive/tar\"\n\
         fn inspect(r *tar.reader) {\n\
             r.close()\n\
         }\n\
         fn main() {}\n",
        "tar.reader",
    );

    rejects_e1002(
        "import \"archive/tar\"\n\
         fn inspect(w *tar.writer) {\n\
             w.next()\n\
         }\n\
         fn main() {}\n",
        "tar.writer",
    );

    rejects_e1002(
        "import \"archive/tar\"\n\
         fn inspect(h *tar.header) {\n\
             h.write_header()\n\
         }\n\
         fn main() {}\n",
        "tar.header",
    );

    rejects_e1002(
        "import \"archive/tar\"\n\
         fn main() {\n\
             tr := tar.new_reader(os.stdin)\n\
             tr.bogus_tar_method()\n\
         }\n",
        "tar.reader",
    );
}

#[test]
fn real_gzip_and_tar_methods_accept() {
    accepts(
        "import \"compress/gzip\"\n\
         import \"archive/tar\"\n\
         fn use_gzip(r *gzip.reader, w *gzip.writer, buf []byte) {\n\
             r.multistream(false)\n\
             _ = r.read(buf)\n\
             _ = r.reset(os.stdin)\n\
             _ = r.close()\n\
             _ = w.write(buf)\n\
             _ = w.flush()\n\
             w.reset(os.stdout)\n\
             _ = w.close()\n\
         }\n\
         fn use_tar(tr *tar.reader, tw *tar.writer, h *tar.header, buf []byte) {\n\
             _ = tr.next()\n\
             _ = tr.read(buf)\n\
             _ = tw.write_header(h)\n\
             _ = tw.write(buf)\n\
             _ = tw.flush()\n\
             _ = tw.close()\n\
             _ = h.file_info()\n\
         }\n\
         fn main() {}\n",
    );
}
