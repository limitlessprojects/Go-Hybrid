//! import_allowlist.rs — SPEC M5 curated allowlist (issue #658 gap 2).
//!
//! An `import` line (or a dotted-name package root) outside the curated
//! Go surface used to pass `hyb check` with 0 diagnostics and then fail
//! `go build` with "cannot find module for path", so an accepted Hybrid
//! program would emit unbuildable Go. HYB-E1004 names the allowlist
//! instead (SPEC M5: a clear compile error, not a silent link failure).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn non_curated_import_path_is_e1004() {
    let r = check(
        "import \"math\"\n\
         fn main() {\n\
           fmt.println(1)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1004".to_string()), "{:?}", r.diagnostics);
}

#[test]
fn third_party_import_path_is_e1004() {
    let r = check(
        "import \"github.com/acme/widgets\"\n\
         fn main() {\n\
           fmt.println(1)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1004".to_string()), "{:?}", r.diagnostics);
}

#[test]
fn unknown_dotted_root_without_import_is_e1004() {
    let r = check(
        "fn main() {\n\
           fmt.println(math.sqrt(2.0))\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1004".to_string()), "{:?}", r.diagnostics);
}

#[test]
fn unknown_root_alone_is_e1004() {
    let r = check(
        "fn main() {\n\
           fmt.println(widgets.count)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1004".to_string()), "{:?}", r.diagnostics);
}

#[test]
fn curated_short_imports_stay_accepted() {
    for src in [
        "import \"fmt\"\nfn main() { fmt.println(1) }\n",
        "import \"strings\"\nfn main() { fmt.println(strings.to_upper(\"x\")) }\n",
        "import \"sync\"\nstruct pool { mu sync.mutex }\nfn main() {\n  p := pool{}\n  _ = p\n}\n",
        "import \"sync/atomic\"\nfn main() {\n  var hits atomic.int64\n  _ = hits\n}\n",
        "import \"net/http\"\nfn main() { fmt.println(http.status) }\n",
        "import \"encoding/hex\"\nfn main() { fmt.println(hex.encode(\"ab\")) }\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{src}\n{:?}", r.diagnostics);
    }
}

#[test]
fn rust_bridge_imports_stay_accepted() {
    for src in [
        "import \"rust/serde_json\"\nfn main() { fmt.println(1) }\n",
        "import \"rust/flate2\"\nfn main() { fmt.println(1) }\n",
        "import \"rust/sha3\"\nfn main() { fmt.println(1) }\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{src}\n{:?}", r.diagnostics);
    }
}

#[test]
fn builtin_roots_without_import_stay_accepted() {
    // spike-10's shape: a curated root used with no import line at all —
    // the emitter's usage-driven import collection maps it.
    let r = check(
        "fn main() {\n\
           fmt.println(hex.encode(\"ab\"))\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn imported_package_roots_pass_field_access() {
    let r = check(
        "import \"net/http\"\n\
         fn main() {\n\
           fmt.println(http.status)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn local_struct_field_access_is_not_e1004() {
    let r = check(
        "struct req { url string }\n\
         fn main() {\n\
           r := req{url: \"x\"}\n\
           fmt.println(r.url)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn bare_serde_json_import_is_e1004_rust_prefix_required() {
    // `import "serde_json"` would emit Go `import "serde_json"` — not a
    // resolvable path. The bridge spelling is `rust/serde_json` (M6).
    let r = check(
        "import \"serde_json\"\n\
         fn main() {\n\
           fmt.println(1)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1004".to_string()), "{:?}", r.diagnostics);
}

#[test]
fn e1004_message_names_the_allowlist() {
    let r = check(
        "import \"math\"\n\
         fn main() {\n\
           fmt.println(1)\n\
         }\n",
    );
    let e1004 = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1004")
        .expect("E1004 present");
    let text = format!("{} {:?}", e1004.message, e1004.notes);
    assert!(text.contains("curated"), "message must name the allowlist: {text}");
    assert!(text.contains("Decision Log"), "note must point at the growth path: {text}");
}
