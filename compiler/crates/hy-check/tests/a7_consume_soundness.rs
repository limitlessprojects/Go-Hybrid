use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test must parse");
    check_file(&ast)
}

#[test]
fn consume_rejects_non_owned_parameter_handle() {
    let src = "
import \"rust/flate2\"
fn helper(w flate2.writer) (err error) {
    _ = w.finish()?
    return nil
}
fn main() (err error) {
    w := flate2.new_writer()?
    helper(w)?
    _ = w.finish()?
    return nil
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when consuming non-owned parameter `w`, got {:?}",
        r.diagnostics
    );
}

#[test]
fn package_consume_freezes_composite_literal_and_call_handle_roots() {
    let src = "
import \"rust/flate2\"
struct holder { w flate2.writer }
fn main() (err error) {
    w := flate2.new_writer()?
    _ = flate2.finish((holder{w: w}).w)?
    _ = flate2.finish((holder{w: w}).w)?
    return nil
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on second consume of `w` via composite literal field, got {:?}",
        r.diagnostics
    );
}

#[test]
fn method_consume_freezes_composite_literal_and_call_handle_roots() {
    let src = "
import \"rust/flate2\"
struct holder { w flate2.writer }
fn main() (err error) {
    w := flate2.new_writer()?
    _ = ((holder{w: w}).w).finish()?
    _ = ((holder{w: w}).w).finish()?
    return nil
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on second method consume of `w` via composite literal field, got {:?}",
        r.diagnostics
    );
}
