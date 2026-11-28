//! Regression tests for issue #4190: keyword-name checks on enum declarations,
//! dotted associated-function names, and non-Rust package import aliases.
//! All three surfaces reach hy-emit as identifiers and previously bypassed
//! HYB-E1002 (`"box.default" != "default"`, `Decl::Enum(_)` skipped,
//! `Decl::Import` unhandled), so accepted programs failed `hyb build`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn assert_rejects(src: &str, what: &str) {
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}\ngot {errs:?}");
    let needle = format!("{what} named");
    assert!(
        errs.iter()
            .any(|e| e.starts_with("HYB-E1002") && e.contains(&needle)),
        "expected HYB-E1002 `{needle}` for:\n{src}\ngot {errs:?}"
    );
}

#[test]
fn enum_declarations_named_default_or_none_are_rejected() {
    assert_rejects("enum default { ready, off }\nfn main() {}\n", "enum");
    assert_rejects("enum none { ready, off }\nfn main() {}\n", "enum");
}

#[test]
fn enum_variant_names_are_rejected() {
    assert_rejects("enum state { default, off }\nfn main() {}\n", "enum variant");
    assert_rejects("enum state { none, off }\nfn main() {}\n", "enum variant");
}

#[test]
fn enum_payload_types_are_traversed() {
    // payload fn type with a `default` parameter
    assert_rejects(
        "enum action { run(fn(default int) int) }\nfn main() {}\n",
        "parameter",
    );
    // payload slice of struct with a `none` field
    assert_rejects(
        "struct p { none int }\nenum packet { buf([p]int) }\nfn main() {}\n",
        "struct field",
    );
}

#[test]
fn dotted_associated_function_names_are_rejected() {
    // receiver-associated: fn box.default() — both segments checked
    assert_rejects(
        "struct box { v int }\nfn box.default() box { return box{v: 1} }\nfn main() {}\n",
        "fn",
    );
    assert_rejects(
        "struct box { v int }\nfn box.none() box { return box{v: 1} }\nfn main() {}\n",
        "fn",
    );
    // the receiver TYPE is the dotted head, not a declaration site —
    // `fn default.build()` declares a method on type `default` (rejected)
    assert_rejects(
        "struct d { n int }\nfn default.build(x d) int { return x.n }\nfn main() {}\n",
        "fn",
    );
}

#[test]
fn dotted_names_with_clean_segments_stay_accepted() {
    // associated fn on a type named `defaults` — segment `defaults` != `default`
    let r = check(
        "struct box { v int }\nfn box.defaults() box { return box{v: 1} }\nfn main() { fmt.println(\"{box{v: 1}}\") }\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "must accept:\n got {:?}",
        errors(&r)
    );
}

#[test]
fn import_aliases_are_rejected() {
    // last path segment becomes the Go package identifier
    assert_rejects(
        "import \"example.com/x/default\"\nfn main() {}\n",
        "package import",
    );
    assert_rejects("import \"example.com/y/none\"\nfn main() {}\n", "package import");
    assert_rejects("import \"default\"\nfn main() {}\n", "package import");
}

#[test]
fn rust_imports_and_clean_aliases_stay_accepted() {
    for src in [
        // rust/ crates bypass the trailing-segment registration rule
        "import \"rust/serde_json\"\nfn main() {\n    v := serde_json.new_object()\n    _ = v\n}\n",
        // clean trailing segments
        "import \"example.com/x/fallback\"\nfn main() {}\n",
        "import \"fmt\"\nfn main() {\n    fmt.println(\"ok\")\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "must accept:\n{src}\ngot {:?}", errors(&r));
    }
}
