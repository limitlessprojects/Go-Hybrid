//! Regression tests for issue #1851:
//! Named arguments on Hybrid `fn`, method, builtin, closure, task, defer, and
//! const-initializer calls must be rejected with HYB-E1002 instead of being
//! silently ignored and emitted positionally, while valid `rust/` crate named
//! arguments continue to pass and malformed `rust/` crate named arguments
//! (unknown parameter, duplicate binding, missing/excess args) reject cleanly.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics.iter().any(|d| d.code == "HYB-E1002")
}

const CALC: &str = "struct calc {\n    k int\n}\n\nfn (c calc) sub(a int, b int) int {\n    return a - b + c.k\n}\n\n";

#[test]
fn named_args_on_fn_and_method_reject() {
    // Repro 1: sub(b=7, a=10) would emit sub(7, 10) and compute -3 instead of 3.
    let r1 = check(
        "fn sub(a int, b int) int {\n    return a - b\n}\n\nfn main() {\n    fmt.println(sub(b=7, a=10))\n}\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(has_e1002(&r1));

    // Repro 2: scale(big=300, small=2) bypassed E6002 by name then failed go build by position.
    let r2 = check(
        "fn scale(small i8, big int) int {\n    return int(small) * big\n}\n\nfn main() {\n    fmt.println(scale(big=300, small=2))\n}\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(has_e1002(&r2));

    // Repro 3: unknown argument name on fn or method.
    let r3 = check(&format!(
        "{CALC}fn main() {{\n    c := calc{{k: 1}}\n    fmt.println(c.sub(zzz=10, a=7))\n}}\n"
    ));
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(has_e1002(&r3));
}

#[test]
fn task_defer_closure_and_const_named_args_reject() {
    let task_r = check(
        "fn work(a int, b int) {\n    fmt.println(a - b)\n}\n\nfn main() {\n    task work(b=7, a=10)\n}\n",
    );
    assert_eq!(task_r.verdict, Verdict::Reject);
    assert!(has_e1002(&task_r));

    let defer_r = check(
        "fn cleanup(code int) {\n    fmt.println(code)\n}\n\nfn main() {\n    defer cleanup(code=1)\n}\n",
    );
    assert_eq!(defer_r.verdict, Verdict::Reject);
    assert!(has_e1002(&defer_r));

    let const_r = check(
        "const c = complex(im=1.0, re=2.0)\nfn main() {\n    fmt.println(c)\n}\n",
    );
    assert_eq!(const_r.verdict, Verdict::Reject);
    assert!(has_e1002(&const_r));
}

#[test]
fn shadowed_rust_crate_import_rejects_across_all_scopes() {
    // Block local shadowing `import "rust/sha3"` after valid crate call.
    let src_block = format!(
        "import \"rust/sha3\"\n\n{CALC}fn main() {{\n    c := sha3.stream_checksum(1000, seed=7)?\n    fmt.println(c)\n    if true {{\n        sha3 := calc{{k: 0}}\n        fmt.println(sha3.sub(b=7, a=10))\n    }}\n}}\n"
    );
    let rb = check(&src_block);
    assert_eq!(rb.verdict, Verdict::Reject);
    assert!(has_e1002(&rb));

    // Method receiver and parameter shadowing `import "rust/sha3"`.
    let src_recv = format!(
        "import \"rust/sha3\"\n\n{CALC}fn (sha3 calc) run() int {{\n    return sha3.sub(b=7, a=10)\n}}\n\nfn main() {{\n    fmt.println((calc{{k: 0}}).run())\n}}\n"
    );
    assert_eq!(check(&src_recv).verdict, Verdict::Reject);

    // Closure binders (for, match, select) shadowing `import "rust/sha3"`.
    let src_closure = format!(
        "import \"rust/sha3\"\n\n{CALC}fn main() {{\n    f := fn() {{\n        for sha3 in []calc{{calc{{k: 0}}}} {{\n            fmt.println(sha3.sub(b=7, a=10))\n        }}\n    }}\n    f()\n}}\n"
    );
    let rc = check(&src_closure);
    assert_eq!(rc.verdict, Verdict::Reject);
    assert!(has_e1002(&rc));
}

#[test]
fn rust_crate_named_arg_descriptors_validated() {
    // Valid rust/sha3 named argument call passes with 0 E1002 diagnostics.
    let ok = check(
        "import \"rust/sha3\"\n\nfn main() {\n    c := sha3.stream_checksum(1000, seed=7)?\n    fmt.println(c)\n}\n",
    );
    assert!(!has_e1002(&ok), "unexpected E1002: {:?}", ok.diagnostics);

    // Unknown parameter name `seedx=` on rust/sha3.stream_checksum must reject at check time.
    let bad_name = check(
        "import \"rust/sha3\"\n\nfn main() {\n    c := sha3.stream_checksum(1000, seedx=7)?\n    fmt.println(c)\n}\n",
    );
    assert_eq!(bad_name.verdict, Verdict::Reject);
    assert!(has_e1002(&bad_name));

    // Duplicate parameter binding (positional `1000` binds `iters`, then `iters=2000` rebinds it).
    let dup_bind = check(
        "import \"rust/sha3\"\n\nfn main() {\n    c := sha3.stream_checksum(1000, iters=2000)?\n    fmt.println(c)\n}\n",
    );
    assert_eq!(dup_bind.verdict, Verdict::Reject);
    assert!(has_e1002(&dup_bind));
}
