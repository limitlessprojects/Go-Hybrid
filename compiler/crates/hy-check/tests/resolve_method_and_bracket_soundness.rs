//! Regression tests for `HYB-E1001` (`resolve.rs`):
//! 1. Receiver methods (`fn (b box) compute() int`) must not populate file-level
//!    value/declaration tables: unqualified calls/references (`compute()`, `f := compute`),
//!    package-shadowing method names (`fn (b box) math()`, `fn (b box) http()`), and
//!    same-named self-referential constants (`fn (b box) k()`, `const k = k + 1`)
//!    reject with `HYB-E1001`.
//! 2. Folded bracketed index calls (`fold_type_args` before `(`) on struct fields
//!    (`h.fns[undefined_key](42)`), function-local bindings shadowing top-level
//!    declarations (`id := map[string]fn(int) int{}; id[undefined_key](42)`), and
//!    chained index calls (`m[k1][undefined_k2](42)`) reject with `HYB-E1001`.
//! 3. Multi-segment `MatchPattern::Path` patterns (`undeclared_root.v`, uncurated
//!    package `math.pi`, unknown curated member `http.nonexistent_status`, and
//!    unknown enum variant `color.nonexistent`) reject with `HYB-E1001`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src)
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

fn assert_rejects_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
}

fn assert_no_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1001"),
        "must NOT fire HYB-E1001 for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn receiver_methods_do_not_leak_into_file_value_or_package_tables() {
    // Calling or referencing a struct method without a receiver must reject with HYB-E1001.
    assert_rejects_e1001(
        "struct box {\n    v int\n}\nfn (b box) compute() int {\n    return b.v + 1\n}\nfn main() {\n    x := compute()\n    _ = x\n}\n",
    );
    assert_rejects_e1001(
        "struct box {\n    v int\n}\nfn (b box) compute() int {\n    return b.v + 1\n}\nfn main() {\n    f := compute\n    _ = f\n}\n",
    );
    // Receiver methods named after uncurated or curated Go packages must not poison file-wide local_decls.
    assert_rejects_e1001(
        "struct box {\n    v int\n}\nfn (b box) math() int {\n    return b.v\n}\nfn main() {\n    _ = math.abs(1.0)\n}\n",
    );
    assert_rejects_e1001(
        "import \"net/http\"\nstruct box {\n    v int\n}\nfn (b box) http() int {\n    return b.v\n}\nfn main() {\n    _ = http.nonexistent_export\n}\n",
    );
    // Receiver method sharing a name with a self-referential const must not bypass self-reference E1001.
    assert_rejects_e1001(
        "struct box {\n    v int\n}\nfn (b box) k() int {\n    return b.v\n}\nconst k = k + 1\nfn main() {\n    _ = k\n}\n",
    );

    // Valid receiver calls and associated functions continue to pass without HYB-E1001.
    assert_no_e1001(
        "struct box {\n    v int\n}\nfn (b box) compute() int {\n    return b.v + 1\n}\nfn main() {\n    b := box{v: 4}\n    fmt.println(b.compute())\n}\n",
    );
}

#[test]
fn folded_bracket_index_calls_on_fields_shadowed_locals_and_chains_reject_e1001() {
    // Field-indexed function map call with undeclared key (`h.fns[undefined_key](42)`).
    assert_rejects_e1001(
        "struct holder {\n    fns map[string]fn(int) int\n}\nfn run(h holder) int {\n    return h.fns[undefined_key](42)\n}\nfn main() {}\n",
    );
    // Function-local map shadowing a top-level generic function name (`id[undefined_key](42)`).
    assert_rejects_e1001(
        "fn id[t any](x t) t {\n    return x\n}\nfn run(id map[string]fn(int) int) int {\n    return id[undefined_key](42)\n}\nfn main() {}\n",
    );
    // Indexed struct receiver with field-indexed function map call (`rows[k1].fns[undefined_k2](42)`).
    assert_rejects_e1001(
        "struct holder {\n    fns map[string]fn(int) int\n}\nfn run(rows map[string]holder, k1 string) int {\n    return rows[k1].fns[undefined_k2](42)\n}\nfn main() {}\n",
    );
    // Field selector inside folded bracket (`m[undeclared_holder.key](42)`).
    assert_rejects_e1001(
        "fn run(m map[string]fn(int) int) int {\n    return m[undeclared_holder.key](42)\n}\nfn main() {}\n",
    );

    // Valid field-indexed, shadowed-local-indexed, and nested-field-indexed calls do not fire HYB-E1001.
    assert_no_e1001(
        "struct holder {\n    fns map[string]fn(int) int\n}\nfn id[t any](x t) t {\n    return x\n}\nfn run(h holder, id map[string]fn(int) int, rows map[string]holder, k1 string, k2 string) int {\n    a := h.fns[k1](1)\n    b := id[k1](a)\n    return rows[k1].fns[k2](b)\n}\nfn main() {}\n",
    );
}

#[test]
fn multi_segment_match_path_patterns_reject_unresolved_roots_and_members_e1001() {
    assert_rejects_e1001(
        "fn run(x option[int]) int {\n    match x {\n        undeclared_pkg.val: {\n            return 1\n        }\n        _: {\n            return 0\n        }\n    }\n}\nfn main() {}\n",
    );
    assert_rejects_e1001(
        "fn run(x option[int]) int {\n    match x {\n        math.pi: {\n            return 1\n        }\n        _: {\n            return 0\n        }\n    }\n}\nfn main() {}\n",
    );
    assert_rejects_e1001(
        "import \"net/http\"\nfn run(x option[int]) int {\n    match x {\n        http.nonexistent_status: {\n            return 1\n        }\n        _: {\n            return 0\n        }\n    }\n}\nfn main() {}\n",
    );
    assert_rejects_e1001(
        "enum color {\n    red\n    green\n}\nfn run(c color) int {\n    match c {\n        color.nonexistent: {\n            return 1\n        }\n        _: {\n            return 0\n        }\n    }\n}\nfn main() {}\n",
    );
    assert_no_e1001(
        "enum color {\n    red\n    green\n}\nfn run(c color) int {\n    match c {\n        color.red: {\n            return 1\n        }\n        color.green: {\n            return 2\n        }\n    }\n}\nfn main() {}\n",
    );
}
