//! Soundness and buildable validation tests for `path` and `path/filepath` calls under Row 13 (BUILDABLE):
//! 1. `path.err_bad_pattern`, `filepath.err_bad_pattern`, `filepath.skip_dir`, and `filepath.skip_all`
//!    are package error variables, not functions; calls must reject fail-closed with `HYB-E1002`.
//! 2. `path` core functions require exact arity:
//!    - 1 arg: base, clean, dir, ext, is_abs, split.
//! 3. `filepath` core functions require exact arity:
//!    - 1 arg: abs, base, clean, dir, eval_symlinks, ext, from_slash, glob, has_prefix, is_abs,
//!      is_local, split, split_list, to_slash, volume_name.
//!    - 2 args: rel.
//!    Arity mismatches must reject fail-closed with `HYB-E1002`.
//! 4. Positive controls: legal calls with proper arities accept cleanly.
//!
//! Settlement Wallet (Base L2 USDC): 0x8366bCe3a2D379De7656D7A67015789FaF999f20

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

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_path_and_filepath_error_variables_called_as_functions_reject() {
    // path.err_bad_pattern
    rejects_e1002("import \"path\"\nfn main() {\n    _ = path.err_bad_pattern()\n}\n");

    // filepath.err_bad_pattern, skip_dir, skip_all
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    _ = filepath.err_bad_pattern()\n}\n");
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    _ = filepath.skip_dir()\n}\n");
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    _ = filepath.skip_all()\n}\n");
}

#[test]
fn test_path_function_arity_mismatches_reject() {
    // clean: takes 1
    rejects_e1002("import \"path\"\nfn main() {\n    s := path.clean()\n    _ = s\n}\n");
    rejects_e1002("import \"path\"\nfn main() {\n    s := path.clean(\"a\", \"b\")\n    _ = s\n}\n");

    // dir: takes 1
    rejects_e1002("import \"path\"\nfn main() {\n    s := path.dir()\n    _ = s\n}\n");

    // base: takes 1
    rejects_e1002("import \"path\"\nfn main() {\n    s := path.base()\n    _ = s\n}\n");

    // is_abs: takes 1
    rejects_e1002("import \"path\"\nfn main() {\n    b := path.is_abs()\n    _ = b\n}\n");

    // split: takes 1
    rejects_e1002("import \"path\"\nfn main() {\n    d, f := path.split()\n    _ = d\n    _ = f\n}\n");
}

#[test]
fn test_filepath_function_arity_mismatches_reject() {
    // abs: takes 1
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    a, err := filepath.abs()\n    _ = a\n    _ = err\n}\n");
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    a, err := filepath.abs(\"a\", \"b\")\n    _ = a\n    _ = err\n}\n");

    // clean: takes 1
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    s := filepath.clean()\n    _ = s\n}\n");

    // dir: takes 1
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    s := filepath.dir()\n    _ = s\n}\n");

    // split: takes 1
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    d, f := filepath.split()\n    _ = d\n    _ = f\n}\n");

    // glob: takes 1
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    m, err := filepath.glob()\n    _ = m\n    _ = err\n}\n");

    // rel: takes 2 (basepath, targpath)
    rejects_e1002("import \"path/filepath\"\nfn main() {\n    r, err := filepath.rel(\"/a\")\n    _ = r\n    _ = err\n}\n");
}

#[test]
fn test_path_and_filepath_positive_controls_accept() {
    let src = r#"import "path"
import "path/filepath"

fn main() {
    // Legal path calls
    p1 := path.clean("/a/b/../c")
    _ = p1

    p2 := path.dir("/a/b/c")
    _ = p2

    p3 := path.base("/a/b/c")
    _ = p3

    p4 := path.ext("main.go")
    _ = p4

    b1 := path.is_abs("/a/b")
    _ = b1

    d1, f1 := path.split("/a/b/c.txt")
    _ = d1
    _ = f1

    // Legal filepath calls
    a1, err2 := filepath.abs("main.go")
    _ = a1
    _ = err2

    fp1 := filepath.clean("./a/b")
    _ = fp1

    fp2 := filepath.dir("./a/b/c")
    _ = fp2

    d2, f2 := filepath.split("./a/b/c.txt")
    _ = d2
    _ = f2

    rel, err3 := filepath.rel("/a", "/a/b/c")
    _ = rel
    _ = err3
}
"#;
    accepts(src);
}
