//! Tests for curated foreign method surfaces of standard library `io` interfaces
//! (HYB-E1002, Row 13 BUILDABLE, issue #4079).
//!
//! Guarantees that invoking phantom or cross-interface methods on `io.reader`,
//! `io.writer`, `io.closer`, `io.seeker`, composite `io` interfaces, and
//! byte/rune/string/offset `io` interfaces rejects fail-closed with HYB-E1002,
//! while their real Go 1.26 method surfaces accept cleanly with 0 diagnostics.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_and_cross_interface_methods_on_io_core_interfaces_rejected() {
    for (decl, call) in [
        ("r io.reader", "r.nonexistent_io_method()"),
        ("r io.reader", "r.write([]byte{})"),
        ("r io.reader", "r.close()"),
        ("r io.reader", "r.seek(0, 0)"),
        ("w io.writer", "w.nonexistent_io_method()"),
        ("w io.writer", "w.read([]byte{})"),
        ("w io.writer", "w.close()"),
        ("c io.closer", "c.nonexistent_io_method()"),
        ("c io.closer", "c.read([]byte{})"),
        ("c io.closer", "c.write([]byte{})"),
        ("s io.seeker", "s.read([]byte{})"),
        ("s io.seeker", "s.close()"),
        ("rc io.read_closer", "rc.write([]byte{})"),
        ("rc io.read_closer", "rc.seek(0, 0)"),
        ("wc io.write_closer", "wc.read([]byte{})"),
        ("rw io.read_writer", "rw.close()"),
        ("rwc io.read_write_closer", "rwc.seek(0, 0)"),
        ("rs io.read_seeker", "rs.write([]byte{})"),
        ("rs io.read_seeker", "rs.close()"),
        ("rsc io.read_seek_closer", "rsc.write([]byte{})"),
        ("ws io.write_seeker", "ws.read([]byte{})"),
        ("ws io.write_seeker", "ws.close()"),
        ("rws io.read_write_seeker", "rws.close()"),
    ] {
        let bad = check(&format!(
            "fn use_io({decl}) {{\n\
                 {call}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        ));
        assert_eq!(
            bad.verdict,
            Verdict::Reject,
            "expected `{call}` on `{decl}` to reject, got {:?}",
            bad.diagnostics
        );
        assert!(
            codes(&bad).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{call}` on `{decl}`, got {:?}",
            codes(&bad)
        );
    }
}

#[test]
fn phantom_and_cross_interface_methods_on_io_specialized_interfaces_rejected() {
    for (decl, call) in [
        ("rf io.reader_from", "rf.read([]byte{})"),
        ("wt io.writer_to", "wt.write([]byte{})"),
        ("ra io.reader_at", "ra.read([]byte{})"),
        ("wa io.writer_at", "wa.write([]byte{})"),
        ("br io.byte_reader", "br.unread_byte()"),
        ("bs io.byte_scanner", "bs.write_byte(1)"),
        ("bw io.byte_writer", "bw.read_byte()"),
        ("rr io.rune_reader", "rr.unread_rune()"),
        ("rs io.rune_scanner", "rs.read_byte()"),
        ("sw io.string_writer", "sw.write([]byte{})"),
    ] {
        let bad = check(&format!(
            "fn use_io({decl}) {{\n\
                 {call}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        ));
        assert_eq!(
            bad.verdict,
            Verdict::Reject,
            "expected `{call}` on `{decl}` to reject, got {:?}",
            bad.diagnostics
        );
        assert!(
            codes(&bad).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{call}` on `{decl}`, got {:?}",
            codes(&bad)
        );
    }
}

#[test]
fn pointer_and_compact_alias_io_receivers_enforce_surface() {
    for decl in [
        "h *io.reader",
        "h *io.writer",
        "h *io.closer",
        "h io.readcloser",
        "h *io.read_closer",
        "h io.writecloser",
        "h io.readwriter",
        "h io.readwritecloser",
        "h io.readseeker",
        "h io.readseekcloser",
        "h io.writeseeker",
        "h io.readwriteseeker",
        "h io.bytereader",
        "h io.bytescanner",
        "h io.stringwriter",
    ] {
        let bad = check(&format!(
            "fn use_io({decl}) {{\n\
                 h.bogus_io_method()\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        ));
        assert_eq!(
            bad.verdict,
            Verdict::Reject,
            "expected `{decl}` phantom method to reject, got {:?}",
            bad.diagnostics
        );
        assert!(
            codes(&bad).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{decl}`, got {:?}",
            codes(&bad)
        );
    }
}

#[test]
fn package_level_io_functions_rejected_as_methods() {
    for (decl, call) in [
        ("r io.reader", "r.read_all()"),
        ("r io.reader", "r.read_full([]byte{})"),
        ("w io.writer", "w.copy()"),
        ("w io.writer", "w.write_string(\"hello\")"),
    ] {
        let bad = check(&format!(
            "fn use_io({decl}) {{\n\
                 {call}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        ));
        assert_eq!(
            bad.verdict,
            Verdict::Reject,
            "expected `{call}` on `{decl}` to reject, got {:?}",
            bad.diagnostics
        );
        assert!(
            codes(&bad).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{call}` on `{decl}`, got {:?}",
            codes(&bad)
        );
    }
}

#[test]
fn real_io_interface_methods_accept() {
    let good = check(
        "fn use_core(\n\
             r io.reader,\n\
             w io.writer,\n\
             c io.closer,\n\
             s io.seeker,\n\
             rw io.read_writer,\n\
             rc io.read_closer,\n\
             wc io.write_closer,\n\
             rwc io.read_write_closer,\n\
             rs io.read_seeker,\n\
             rsc io.read_seek_closer,\n\
             ws io.write_seeker,\n\
             rws io.read_write_seeker,\n\
         ) error {\n\
             buf := []byte{}\n\
             _ = r.read(buf)?\n\
             _ = w.write(buf)?\n\
             c.close()?\n\
             _ = s.seek(0, 0)?\n\
             _ = rw.read(buf)?\n\
             _ = rw.write(buf)?\n\
             _ = rc.read(buf)?\n\
             rc.close()?\n\
             _ = wc.write(buf)?\n\
             wc.close()?\n\
             _ = rwc.read(buf)?\n\
             _ = rwc.write(buf)?\n\
             rwc.close()?\n\
             _ = rs.read(buf)?\n\
             _ = rs.seek(0, 0)?\n\
             _ = rsc.read(buf)?\n\
             _ = rsc.seek(0, 0)?\n\
             rsc.close()?\n\
             _ = ws.write(buf)?\n\
             _ = ws.seek(0, 0)?\n\
             _ = rws.read(buf)?\n\
             _ = rws.write(buf)?\n\
             _ = rws.seek(0, 0)?\n\
             return nil\n\
         }\n\
         fn use_specialized(\n\
             r io.reader,\n\
             w io.writer,\n\
             rf io.reader_from,\n\
             wt io.writer_to,\n\
             ra io.reader_at,\n\
             wa io.writer_at,\n\
             br io.byte_reader,\n\
             bs io.byte_scanner,\n\
             bw io.byte_writer,\n\
             rr io.rune_reader,\n\
             rsc io.rune_scanner,\n\
             sw io.string_writer,\n\
         ) error {\n\
             buf := []byte{}\n\
             _ = rf.read_from(r)?\n\
             _ = wt.write_to(w)?\n\
             _ = ra.read_at(buf, 0)?\n\
             _ = wa.write_at(buf, 0)?\n\
             _ = br.read_byte()?\n\
             _ = bs.read_byte()?\n\
             bs.unread_byte()?\n\
             bw.write_byte(1)?\n\
             _, _ = rr.read_rune()?\n\
             _, _ = rsc.read_rune()?\n\
             rsc.unread_rune()?\n\
             _ = sw.write_string(\"ok\")?\n\
             return nil\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got {:?}",
        good.diagnostics
    );
    assert!(
        good.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        good.diagnostics
    );
}
