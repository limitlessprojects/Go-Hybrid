//! named_call_args.rs: named arguments are only supported on rust/ crate
//! calls. On a Hybrid fn or method the emitter would drop the name and pass
//! by position, so the checker rejects them (HYB-E1002, issue #1851).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn e1002_count(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002" && d.message.contains("named argument"))
        .count()
}

#[test]
fn named_args_on_fn_reject() {
    let src = "fn sub(a int, b int) int {\n    return a - b\n}\n\nfn main() {\n    fmt.println(sub(b=7, a=10))\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(e1002_count(&r) >= 1);
}

#[test]
fn unknown_arg_name_rejects() {
    let src = "fn sub(a int, b int) int {\n    return a - b\n}\n\nfn main() {\n    fmt.println(sub(zzz=10, a=7))\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(e1002_count(&r) >= 1);
}

#[test]
fn named_args_on_method_reject() {
    let src = "struct counter {\n    n int\n}\n\nfn (c counter) sub(a int, b int) int {\n    return a - b\n}\n\nfn main() {\n    c := counter{n: 1}\n    fmt.println(c.sub(b=7, a=10))\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(e1002_count(&r) >= 1);
}

#[test]
fn positional_args_still_accepted() {
    let src = "fn sub(a int, b int) int {\n    return a - b\n}\n\nfn main() {\n    fmt.println(sub(10, 7))\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert_eq!(e1002_count(&r), 0);
}

#[test]
fn rust_crate_named_args_still_accepted() {
    let src = "import \"rust/sha3\"\n\nfn main() {\n    fmt.println(sha3.stream_checksum(1000, seed=7))\n}\n";
    let r = check(src);
    assert_eq!(e1002_count(&r), 0);
}

const CALC: &str = "struct calc {\n    k int\n}\n\nfn (c calc) sub(a int, b int) int {\n    return a - b + c.k\n}\n\n";

#[test]
fn local_named_like_crate_rejects() {
    let src = format!("{CALC}fn main() {{\n    regex := calc{{k: 0}}\n    fmt.println(regex.sub(b=7, a=10))\n}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(e1002_count(&r) >= 1);
}

#[test]
fn block_local_shadowing_rust_import_rejects() {
    let src = format!("import \"rust/sha3\"\n\n{CALC}fn main() {{\n    fmt.println(sha3.digest256(\"x\", seed=1))\n    if true {{\n        sha3 := calc{{k: 0}}\n        fmt.println(sha3.sub(b=7, a=10))\n    }}\n}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(e1002_count(&r), 1);
}

#[test]
fn param_named_like_import_rejects() {
    let src = format!("import \"rust/sha3\"\n\n{CALC}fn run(sha3 calc) int {{\n    return sha3.sub(b=7, a=10)\n}}\n\nfn main() {{\n    fmt.println(run(calc{{k: 0}}))\n}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(e1002_count(&r) >= 1);
}

#[test]
fn unimported_crate_name_is_not_special() {
    let src = "fn main() {\n    fmt.println(regex.digest256(\"x\", seed=1))\n}\n";
    let r = check(src);
    assert!(e1002_count(&r) >= 1);
}

#[test]
fn closure_for_var_shadowing_rust_import_rejects() {
    let src = format!("import \"rust/sha3\"\n\n{CALC}fn main() {{\n    fmt.println(sha3.digest256(\"x\", seed=1))\n    f := fn() {{\n        for sha3 in []calc{{calc{{k: 0}}}} {{\n            fmt.println(sha3.sub(b=7, a=10))\n        }}\n    }}\n    f()\n}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(e1002_count(&r), 1);
}

#[test]
fn closure_match_binding_shadowing_rust_import_rejects() {
    let src = format!("import \"rust/sha3\"\n\n{CALC}fn pick() option[calc] {{\n    return some(calc{{k: 0}})\n}}\n\nfn main() {{\n    fmt.println(sha3.digest256(\"x\", seed=1))\n    f := fn() {{\n        match pick() {{\n            some(sha3): fmt.println(sha3.sub(b=7, a=10))\n            none: fmt.println(\"none\")\n        }}\n    }}\n    f()\n}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(e1002_count(&r), 1);
}

#[test]
fn closure_select_recv_shadowing_rust_import_rejects() {
    let src = format!("import \"rust/sha3\"\n\n{CALC}fn main() {{\n    fmt.println(sha3.digest256(\"x\", seed=1))\n    ch := make(chan calc, 1)\n    ch <- calc{{k: 0}}\n    f := fn() {{\n        select {{\n            sha3 := <-ch:\n                fmt.println(sha3.sub(b=7, a=10))\n        }}\n    }}\n    f()\n}}\n");
    let r = check(&src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(e1002_count(&r), 1);
}

#[test]
fn closure_unshadowed_rust_import_still_accepted() {
    let src = "import \"rust/sha3\"\n\nfn main() {\n    f := fn() {\n        for i in 0..2 {\n            fmt.println(sha3.stream_checksum(1000, seed=i))\n        }\n    }\n    f()\n}\n";
    let r = check(src);
    assert_eq!(e1002_count(&r), 0);
}
