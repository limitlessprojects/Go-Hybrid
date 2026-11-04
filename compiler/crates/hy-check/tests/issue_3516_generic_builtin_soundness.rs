//! Issue #3516: reject unconstrained generic type parameter operands to
//! built-in functions clear, close, delete, cap, len, append, and copy
//! (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<(String, String)> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect()
}

fn assert_e2005(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let errs = errors(&r);
    assert!(
        errs.iter().any(|(code, _)| code == "HYB-E2005"),
        "expected HYB-E2005 for {what}, got {errs:?}"
    );
}

#[test]
fn rejects_generic_type_parameter_to_clear() {
    let src = r#"
package main

fn wipe[t any](x t) {
    clear(x)
}

fn main() {}
"#;
    assert_e2005(src, "clear on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_to_close() {
    let src = r#"
package main

fn shut[t any](ch t) {
    close(ch)
}

fn main() {}
"#;
    assert_e2005(src, "close on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_to_delete() {
    let src = r#"
package main

fn del[t any](m t) {
    delete(m, "k")
}

fn main() {}
"#;
    assert_e2005(src, "delete on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_to_cap() {
    let src = r#"
package main

fn get_cap[t any](c t) int {
    return cap(c)
}

fn main() {}
"#;
    assert_e2005(src, "cap on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_to_len() {
    let src = r#"
package main

fn get_len[t any](x t) int {
    return len(x)
}

fn main() {}
"#;
    assert_e2005(src, "len on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_to_append() {
    let src = r#"
package main

fn app[t any](s t) {
    _ = append(s, 1)
}

fn main() {}
"#;
    assert_e2005(src, "append on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_to_copy() {
    let src1 = r#"
package main

fn cp1[t any](dst t, src []int) {
    _ = copy(dst, src)
}

fn main() {}
"#;
    assert_e2005(src1, "copy dst on generic type parameter");

    let src2 = r#"
package main

fn cp2[t any](dst []int, src t) {
    _ = copy(dst, src)
}

fn main() {}
"#;
    assert_e2005(src2, "copy src on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_to_min_max() {
    let src1 = r#"
package main

fn test_min[t any](a t, b t) {
    _ = min(a, b)
}

fn main() {}
"#;
    assert_e2005(src1, "min on generic type parameter");

    let src2 = r#"
package main

fn test_max[t any](a t, b t) {
    _ = max(a, b)
}

fn main() {}
"#;
    assert_e2005(src2, "max on generic type parameter");
}

#[test]
fn rejects_generic_type_parameter_via_local_and_field() {
    let src = r#"
package main

fn test_local[t any](x t) {
    y := x
    clear(y)
}

fn main() {}
"#;
    assert_e2005(src, "clear on local inferred generic type parameter");
}

#[test]
fn accepts_generic_slices_maps_channels() {
    let src = r#"
package main

fn test_valid[t any](s []t, m map[string]t, ch chan t) {
    clear(s)
    clear(m)
    delete(m, "k")
    close(ch)
    _ = len(s)
    _ = len(m)
    _ = len(ch)
    _ = cap(s)
    _ = cap(ch)
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for concrete generic containers, got {:?}",
        r.diagnostics
    );
}
