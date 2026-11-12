//! Edge cases for the `bytes.buffer` and `strings.builder` method surfaces
//! beyond `tests/issue_bytes_strings_foreign_surface.rs`: cross-surface bleed,
//! name-mangled phantoms, closure-body phantoms, and builtin lookalikes. All
//! must reject fail-closed with HYB-E1002 (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

fn rejects_e1002(src: &str, label: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{label}: {:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E1002"),
        "{label}: expected HYB-E1002, got {:?}",
        codes(&r)
    );
}

#[test]
fn buffer_only_methods_rejected_on_strings_builder() {
    for m in ["truncate", "unread_byte", "read_byte", "next"] {
        rejects_e1002(
            &format!(
                "import \"strings\"\nfn inspect(b *strings.builder) {{\n b.{m}()\n}}\n"
            ),
            m,
        );
    }
}

#[test]
fn prefix_suffix_and_case_phantoms_rejected() {
    for m in ["write_extra", "rewrite", "stringify", "Write"] {
        rejects_e1002(
            &format!(
                "import \"strings\"\nfn inspect(b *strings.builder) {{\n b.{m}()\n}}\n"
            ),
            m,
        );
    }
}

#[test]
fn phantom_inside_closure_body_rejected() {
    rejects_e1002(
        "import \"bytes\"\nfn inspect(b *bytes.buffer) {\n f := fn() {\n b.totally_fake()\n }\n f()\n}\n",
        "closure body",
    );
}

#[test]
fn builtin_lookalike_rejected() {
    rejects_e1002(
        "import \"bytes\"\nfn inspect(b *bytes.buffer) {\n b.append()\n}\n",
        "append lookalike",
    );
}
