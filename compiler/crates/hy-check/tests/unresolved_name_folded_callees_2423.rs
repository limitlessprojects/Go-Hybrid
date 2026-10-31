//! unresolved_name_folded_callees_2423.rs — regression for issue #2423:
//! `HYB-E1001` (`resolve.rs`) missed undeclared identifiers in three shapes that
//! `parser.rs` folds before the pass sees them, so each reached `go build` with
//! `undefined: X` after `hyb check` reported 0 diagnostics:
//!
//! 1. A parameter or local binding that *shadows* a `PREDECLARED` name used as an
//!    indexed callee (`copy[undef_key](42)`): the bracketed part was skipped
//!    whenever the head was any file-level name, and `collect_file_decl_names`
//!    seeds every predeclared name, so a local `copy` read as a type argument.
//! 2. A bracketed argument folded onto a field name (`r.header[undef](42)`,
//!    `http.status_ok[undef]()`): only `base` was walked, and the curated-member
//!    test matched the member by prefix, so the folded key was never a use.
//! 3. A qualified `match` arm root (`undef_enum.variant:`): only single-segment
//!    paths were checked, so segment 0 (the enum) was skipped.
//!
//! The controls keep the over-approximating behaviour intact: declared index
//! keys, explicit generic instantiation, bare curated members, and enum variant
//! values in arm patterns stay accepted.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

fn rejects_with_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1001"),
        "must NOT fire HYB-E1001 for {:?}, got {:?}",
        src,
        cs
    );
}

// ---------------------------------------------------------------- reject:

#[test]
fn shadowed_predeclared_head_index_key_rejects() {
    // `copy` is a PREDECLARED builtin AND a parameter here, so the folded
    // bracket is an index whose key must resolve.
    rejects_with_e1001(
        "fn run(copy map[string]fn(int)) {\n    copy[undef_shadow_key](42)\n}\nfn main() {\n}\n",
    );
}

#[test]
fn shadowed_predeclared_local_head_index_key_rejects() {
    rejects_with_e1001(
        "fn main() {\n    len := map[string]fn(int){}\n    len[undef_local_key](42)\n}\n",
    );
}

#[test]
fn folded_field_index_key_rejects() {
    rejects_with_e1001(
        "import \"net/http\"\nfn run(r *http.request) {\n    r.header[undef_field_key](42)\n}\nfn main() {\n}\n",
    );
}

#[test]
fn folded_curated_member_index_key_rejects() {
    rejects_with_e1001(
        "import \"net/http\"\nfn main() {\n    _ = http.status_ok[undef_http_t]()\n}\n",
    );
}

#[test]
fn qualified_match_arm_root_rejects() {
    rejects_with_e1001(
        "fn main() {\n    var c int = 1\n    match c {\n        undef_enum.variant: fmt.println(\"x\")\n        _: fmt.println(\"y\")\n    }\n}\n",
    );
}

// ---------------------------------------------------------------- accept:

#[test]
fn declared_index_key_on_shadowing_head_allowed() {
    accepts_without_e1001(
        "fn run(copy map[string]fn(int)) {\n    k := \"a\"\n    copy[k](42)\n}\nfn main() {\n}\n",
    );
}

#[test]
fn explicit_generic_instantiation_allowed() {
    accepts_without_e1001(
        "fn id[t any](x t) t {\n    return x\n}\nfn main() {\n    a := id[int](42)\n    _ = a\n}\n",
    );
}

#[test]
fn bare_curated_member_allowed() {
    accepts_without_e1001("import \"net/http\"\nfn main() {\n    _ = http.status_ok\n}\n");
}

#[test]
fn enum_variant_value_in_arm_pattern_allowed() {
    accepts_without_e1001(
        "enum color {\n    red\n    green\n}\nfn main() {\n    var c color = red\n    match c {\n        red: fmt.println(\"r\")\n        _: fmt.println(\"o\")\n    }\n}\n",
    );
}
