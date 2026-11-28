//! Tests for curated foreign method surfaces of `os.FileInfo` and `os.DirEntry`
//! (HYB-E1002, Row 13 BUILDABLE, issue #4077).
//!
//! Guarantees that invoking phantom/unsupported methods on `os.file_info` or
//! `os.dir_entry` receivers rejects fail-closed with HYB-E1002, while the real
//! Go `io/fs.FileInfo` and `io/fs.DirEntry` method surfaces accept cleanly with
//! 0 diagnostics and preserve downstream method result shapes (`de.info()?` ->
//! `os.file_info`).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_os_file_info_method_rejected() {
    let bad = check(
        "fn inspect(fi os.file_info) {\n\
             fi.nonexistent_file_info_method()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
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
    let msg = &bad.diagnostics[0].message;
    assert!(msg.contains("nonexistent_file_info_method"), "message: {msg}");
    assert!(msg.contains("os.file_info"), "message: {msg}");
}

#[test]
fn phantom_os_dir_entry_method_and_cross_type_methods_rejected() {
    for call in [
        "de.nonexistent_dir_entry_method()",
        "de.size()",
        "de.mod_time()",
        "de.mode()",
        "de.sys()",
    ] {
        let bad = check(&format!(
            "fn inspect_entry(de os.dir_entry) {{\n\
                 {call}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        ));
        assert_eq!(
            bad.verdict,
            Verdict::Reject,
            "expected `{call}` on os.dir_entry to reject, got {:?}",
            bad.diagnostics
        );
        assert!(
            codes(&bad).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{call}`, got {:?}",
            codes(&bad)
        );
    }
}

#[test]
fn pointer_and_compact_alias_receivers_enforce_surface() {
    for decl in [
        "fi *os.file_info",
        "fi os.fileinfo",
        "fi *os.fileinfo",
        "fi *os.dir_entry",
        "fi os.direntry",
        "fi *os.direntry",
    ] {
        let bad = check(&format!(
            "fn inspect({decl}) {{\n\
                 fi.bogus_method()\n\
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
fn package_level_os_functions_rejected_as_methods() {
    for (decl, call) in [
        ("fi os.file_info", "fi.stat()"),
        ("fi os.file_info", "fi.open()"),
        ("de os.dir_entry", "de.read_dir()"),
    ] {
        let bad = check(&format!(
            "fn inspect({decl}) {{\n\
                 {call}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        ));
        assert_eq!(
            bad.verdict,
            Verdict::Reject,
            "expected `{call}` to reject, got {:?}",
            bad.diagnostics
        );
        assert!(
            codes(&bad).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{call}`, got {:?}",
            codes(&bad)
        );
    }
}

#[test]
fn real_os_file_info_and_dir_entry_methods_accept_and_preserve_info_shape() {
    let good = check(
        "fn inspect(fi os.file_info, de os.dir_entry) error {\n\
             _ = fi.name()\n\
             _ = fi.size()\n\
             _ = fi.mode()\n\
             _ = fi.mod_time()\n\
             _ = fi.is_dir()\n\
             _ = fi.sys()\n\
             _ = de.name()\n\
             _ = de.is_dir()\n\
             info := de.info()?\n\
             _ = info.name()\n\
             _ = info.size()\n\
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

    let chained_bad = check(
        "fn inspect(de os.dir_entry) error {\n\
             info := de.info()?\n\
             info.bogus_after_info()\n\
             return nil\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(
        chained_bad.verdict,
        Verdict::Reject,
        "expected phantom method on `de.info()?` result to reject, got {:?}",
        chained_bad.diagnostics
    );
    assert!(
        codes(&chained_bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 on `de.info()?` result, got {:?}",
        codes(&chained_bad)
    );
}
