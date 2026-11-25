//! Soundness tests for SPEC M5 curated package allowlist expansion (log, compress/gzip, archive/tar).
//!
//! Validates:
//! 1. Explicit imports for `log`, `compress/gzip` (and short `gzip`), `archive/tar` (and short `tar`) are accepted
//!    by M5 import resolution and do not reject with HYB-E1001 "outside curated allowlist".
//! 2. Curated package members (e.g. `log.lstdflags`, `gzip.best_compression`, `tar.err_header`) resolve cleanly.
//! 3. Non-existent members of expanded curated packages are rejected with HYB-E1001.
//! 4. Uncurated Go stdlib roots (e.g. `math`, `regexp`, `context`) continue to reject fail-closed with HYB-E1001.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

#[test]
fn curated_allowlist_expansion_imports_and_members_accepted() {
    let r_log = check("import \"log\"\nfn main() {\n    _ = log.lstdflags\n}\n");
    assert_eq!(r_log.verdict, Verdict::Accept, "expected log.lstdflags to pass: {:?}", r_log.diagnostics);

    let r_gzip = check("import \"compress/gzip\"\nfn main() {\n    _ = gzip.best_compression\n}\n");
    assert_eq!(r_gzip.verdict, Verdict::Accept, "expected gzip.best_compression to pass: {:?}", r_gzip.diagnostics);

    let r_tar = check("import \"archive/tar\"\nfn main() {\n    _ = tar.err_header\n}\n");
    assert_eq!(r_tar.verdict, Verdict::Accept, "expected tar.err_header to pass: {:?}", r_tar.diagnostics);

    let r_short_gzip = check("import \"gzip\"\nfn main() {\n    _ = gzip.no_compression\n}\n");
    assert_eq!(r_short_gzip.verdict, Verdict::Accept, "expected gzip.no_compression to pass: {:?}", r_short_gzip.diagnostics);

    let r_short_tar = check("import \"tar\"\nfn main() {\n    _ = tar.err_insecure_path\n}\n");
    assert_eq!(r_short_tar.verdict, Verdict::Accept, "expected tar.err_insecure_path to pass: {:?}", r_short_tar.diagnostics);
}

#[test]
fn non_existent_member_of_expanded_curated_package_rejects() {
    let r_bad_log = check("import \"log\"\nfn main() {\n    _ = log.non_existent_symbol\n}\n");
    assert_eq!(r_bad_log.verdict, Verdict::Reject);
    assert!(
        r_bad_log.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("not a resolved member")),
        "expected HYB-E1001 naming not a resolved member: {:?}",
        r_bad_log.diagnostics
    );
}

#[test]
fn uncurated_stdlib_roots_remain_rejected_fail_closed() {
    let r_math = check("import \"math\"\nfn main() {\n    _ = math.sqrt\n}\n");
    assert_eq!(r_math.verdict, Verdict::Reject);
    assert!(
        r_math.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("curated allowlist")),
        "expected HYB-E1001 naming allowlist for math: {:?}",
        r_math.diagnostics
    );

    let r_regexp = check("import \"regexp\"\nfn main() {\n    _ = regexp.compile\n}\n");
    assert_eq!(r_regexp.verdict, Verdict::Reject);
    assert!(
        r_regexp.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("curated allowlist")),
        "expected HYB-E1001 naming allowlist for regexp: {:?}",
        r_regexp.diagnostics
    );
}
