//! generic_method_subst.rs — regression tests for #1959: methods on a generic
//! struct substitute the receiver's type arguments into parameters and results.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const PRELUDE: &str = "struct container[t] {\n    val t\n}\n\nfn (c *container[t]) put(x t) {\n    c.val = x\n}\n\nfn (c *container[t]) take() t {\n    return c.val\n}\n\n";

#[test]
fn method_argument_uses_receiver_type_argument() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    c := container[int]{{val: 10}}\n    c.put(\"not an int\")\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `put(\"not an int\")` on container[int], got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn method_result_uses_receiver_type_argument() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    c := container[int]{{val: 10}}\n    var s string = c.take()\n    _ = s\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for assigning `take()` on container[int] to string, got {cs:?}"
    );
}

#[test]
fn string_container_rejects_int_argument() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    c := container[string]{{val: \"x\"}}\n    c.put(1)\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for an int argument to container[string].put, got {cs:?}"
    );
}

#[test]
fn well_typed_generic_method_calls_accept() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    c := container[int]{{val: 10}}\n    c.put(20)\n    var n int = c.take()\n    _ = n\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "well-typed container[int] calls must stay accepted, got {cs:?}"
    );
}
