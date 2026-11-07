//! Tests for curated foreign method surfaces of `os.file_info` and `os.dir_entry` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on file info or dir entry handles rejects fail-closed with HYB-E1002,
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
fn phantom_file_info_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             fi os.file_info\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.fi.totally_fake_method()\n\
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
fn pointer_file_info_phantom_method_is_rejected() {
    let bad = check(
        "fn check_fi(fi *os.file_info) {\n\
             fi.phantom_file_metadata()\n\
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
fn phantom_dir_entry_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             de os.dir_entry\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.de.phantom_entry_call()\n\
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
fn os_package_functions_called_as_file_info_methods_are_rejected() {
    for pkg_fn in ["stat", "remove", "chmod", "mkdir"] {
        let src = format!(
            "struct holder {{\n\
                 fi os.file_info\n\
             }}\n\
             fn main() {{\n\
                 var h holder\n\
                 h.fi.{pkg_fn}()\n\
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
fn real_file_info_and_dir_entry_methods_accept() {
    let good = check(
        "struct holder {\n\
             fi os.file_info\n\
             de os.dir_entry\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             _ = h.fi.name()\n\
             _ = h.fi.size()\n\
             _ = h.fi.is_dir()\n\
             _ = h.fi.mod_time()\n\
             _ = h.fi.mode()\n\
             _ = h.fi.sys()\n\
             _ = h.de.name()\n\
             _ = h.de.is_dir()\n\
             _ = h.de.info()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
