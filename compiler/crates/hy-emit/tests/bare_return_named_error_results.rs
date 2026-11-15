use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn bare_return_in_named_error_fn_preserves_assigned_results_and_errors() {
    let src = "\
import \"errors\"

fn compute(bad bool) (total int, err error) {
    total = 42
    if bad {
        err = errors.new(\"boom\")
        return
    }
    return
}

fn mixed(v int) (total int, error) {
    total = v
    return
}

fn main() {
    _, _ = compute(false)
    _, _ = mixed(7)
}
";
    let file = parse_file("test.hy", src).expect("parse ok");
    let go = emit_go(&file).expect("emit ok");
    assert!(
        !go.contains("return 0, nil"),
        "bare return in named-result error fn must not clobber results to zero:\n{go}"
    );
    assert!(
        go.contains("err = errors.New(\"boom\")\n\t\treturn\n"),
        "expected naked return after assigning named error result:\n{go}"
    );
}

#[test]
fn short_explicit_return_and_unnamed_error_fn_still_zero_fill() {
    let src = "\
fn named_short() (total int, err error) {
    return 99
}

fn unnamed_bare() error {
    return
}

fn main() {
    _, _ = named_short()
    _ = unnamed_bare()
}
";
    let file = parse_file("test.hy", src).expect("parse ok");
    let go = emit_go(&file).expect("emit ok");
    assert!(go.contains("return 99, nil"), "short explicit return keeps nil fill:\n{go}");
    assert!(go.contains("func unnamed_bare() error {\n\treturn nil\n}"), "unnamed bare return keeps nil fill:\n{go}");
}
