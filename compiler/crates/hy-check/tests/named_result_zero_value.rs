use hy_check::check_file;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|e| panic!("parse failed: {:?}\nsource:\n{}", e, src));
    check_file(&ast)
}

#[test]
fn named_return_parameter_without_zero_value_rejects_e4002_in_fn_and_closure() {
    let r = check(
        r#"
enum msg {
    data(int)
    quit
}

fn bad_fn() (m msg) {
    return
}
"#,
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E4002"),
        "expected HYB-E4002 on named return parameter `m msg`, got: {:?}",
        r.diagnostics
    );

    let r_closure = check(
        r#"
enum msg {
    data(int)
    quit
}

fn main() {
    f := fn() (m msg) {
        return
    }
    _ = f
}
"#,
    );
    assert!(
        r_closure.diagnostics.iter().any(|d| d.code == "HYB-E4002"),
        "expected HYB-E4002 on closure named return parameter `m msg`, got: {:?}",
        r_closure.diagnostics
    );

    let r_ok = check(
        r#"
enum msg {
    quit
    data(int)
}

fn ok_fn() (m msg) {
    return
}
"#,
    );
    assert!(
        r_ok.diagnostics.is_empty(),
        "expected payload-free first variant enum named return to pass, got: {:?}",
        r_ok.diagnostics
    );
}
