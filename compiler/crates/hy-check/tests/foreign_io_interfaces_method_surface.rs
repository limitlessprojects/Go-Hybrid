//! Tests for curated foreign method surfaces of `io.reader`, `io.writer`, `io.closer`,
//! `io.read_closer`, `io.write_closer`, `io.read_writer`, and `io.seeker` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on standard library I/O interfaces rejects fail-closed with HYB-E1002,
//! while all real Go standard library methods accept cleanly.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_io_reader_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             r io.reader\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.r.totally_fake_method()\n\
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
fn phantom_io_writer_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             w io.writer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.w.phantom_write_call()\n\
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
fn phantom_io_closer_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             c io.closer\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.c.phantom_close_call()\n\
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
fn io_package_functions_called_as_methods_are_rejected() {
    for pkg_fn in ["read_all", "copy", "pipe", "read_full"] {
        let src = format!(
            "struct holder {{\n\
                 r io.reader\n\
             }}\n\
             fn main() {{\n\
                 var h holder\n\
                 h.r.{pkg_fn}()\n\
                 fmt.println(1)\n\
             }}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {pkg_fn}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {pkg_fn}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn real_io_interfaces_methods_accept() {
    let good = check(
        "struct holder {\n\
             r io.reader\n\
             w io.writer\n\
             c io.closer\n\
             rc io.read_closer\n\
             rw io.read_writer\n\
             s io.seeker\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             _ = h.r.read()\n\
             _ = h.w.write()\n\
             _ = h.c.close()\n\
             _ = h.rc.read()\n\
             _ = h.rc.close()\n\
             _ = h.rw.read()\n\
             _ = h.rw.write()\n\
             _ = h.s.seek()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
