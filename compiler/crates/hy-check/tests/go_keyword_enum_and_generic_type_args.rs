//! Regression tests for HYB-E1002 in `go_keyword_names.rs` across:
//! 1. `Decl::Enum` names (`enum default`, `enum none`) and variant payload types
//!    (`enum action { run(fn(default int) int) }`, `enum action { run(fn(none int) int) }`,
//!     `enum packet { buf([fn() int { default := 4\n return default }()]int) }`).
//! 2. `Decl::Fn` associated function names (`fn box.default()`, `fn box.none()`,
//!    `fn default.build()`, `fn none.build()`).
//! 3. `Decl::Import` package roots (`import "default"`, `import "pkg/default"`,
//!    `import "none"`, `import "pkg/none"`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_rejects_e1002(label: &str, src: &str, needle: &str) {
    let ast = parse_file("test.hy", src).unwrap_or_else(|e| panic!("{label}: parse failed: {e:?}"));
    let r = check_file(&ast);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "{label}: expected Reject (HYB-E1002), got Accept (diags={:?})",
        r.diagnostics
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains(needle)),
        "{label}: expected HYB-E1002 containing `{needle}`, got {:?}",
        r.diagnostics
    );
}

fn assert_accepts(label: &str, src: &str) {
    let ast = parse_file("test.hy", src).unwrap_or_else(|e| panic!("{label}: parse failed: {e:?}"));
    let r = check_file(&ast);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "{label}: expected Accept, got {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_enum_named_default_or_none_and_variant_payload_keyword_params() {
    assert_rejects_e1002(
        "enum named default",
        "enum default { ready }\nfn main() {}\n",
        "enum named `default`",
    );
    assert_rejects_e1002(
        "enum named none",
        "enum none { ready }\nfn main() {}\n",
        "enum named `none`",
    );
    assert_rejects_e1002(
        "enum variant payload fn(default int)",
        "enum action { run(fn(default int) int) }\nfn main() {}\n",
        "parameter named `default`",
    );
    assert_rejects_e1002(
        "enum variant payload fn(none int)",
        "enum action { run(fn(none int) int) }\nfn main() {}\n",
        "parameter named `none`",
    );
    assert_rejects_e1002(
        "enum variant payload array length closure default",
        "enum packet { buf([fn() int {\n    default := 4\n    return default\n}()]int) }\nfn main() {}\n",
        "local named `default`",
    );
}

#[test]
fn rejects_associated_fn_and_import_package_root_named_default_or_none() {
    assert_rejects_e1002(
        "associated fn member named default",
        "struct box { x int }\nfn box.default() box { return box{x: 1} }\nfn main() {}\n",
        "fn named `default`",
    );
    assert_rejects_e1002(
        "associated fn member named none",
        "struct box { x int }\nfn box.none() box { return box{x: 1} }\nfn main() {}\n",
        "fn named `none`",
    );
    assert_rejects_e1002(
        "associated fn type named default",
        "fn default.build() int { return 1 }\nfn main() {}\n",
        "fn named `default`",
    );
    assert_rejects_e1002(
        "associated fn type named none",
        "fn none.build() int { return 1 }\nfn main() {}\n",
        "fn named `none`",
    );
    assert_rejects_e1002(
        "import package root named default",
        "import \"default\"\nfn main() {\n    default.run()\n}\n",
        "import named `default`",
    );
    assert_rejects_e1002(
        "import nested path root named default",
        "import \"acme/default\"\nfn main() {\n    default.run()\n}\n",
        "import named `default`",
    );
    assert_rejects_e1002(
        "import package root named none",
        "import \"none\"\nfn main() {\n    none.run()\n}\n",
        "import named `none`",
    );
    assert_rejects_e1002(
        "import nested path root named none",
        "import \"acme/none\"\nfn main() {\n    none.run()\n}\n",
        "import named `none`",
    );
}

#[test]
fn accepts_valid_enums_associated_fns_and_imports() {
    assert_accepts(
        "valid enum and associated fn",
        "import \"fmt\"\nenum action { run(fn(x int) int) }\nstruct box { x int }\nfn box.build(v int) box { return box{x: v} }\nfn main() {\n    b := box.build(7)\n    fmt.println(\"{b.x}\")\n}\n",
    );
}
