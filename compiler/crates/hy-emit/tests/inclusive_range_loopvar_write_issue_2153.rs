//! inclusive_range_loopvar_write_issue_2153.rs
//! Regression test for issue #2153:
//! Inclusive range `lo..=hi`: a write to the loop variable defeats the #322
//! stop flag, so the loop runs past `hi`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn test_inclusive_range_skip_next_idiom() {
    let src = r#"
fn main() {
    xs := [10, 20, 30, 40]
    last := len(xs) - 1
    for i in 0..=last {
        if xs[i] == 30 {
            i += 1
        }
    }
}
"#;
    let go = emit_src(src);
    // The loop post-statement must sample `i < hyHi` simultaneously with increment,
    // so any modification to `i` in the body is evaluated before the next iteration.
    assert!(
        go.contains(", hyCont"),
        "expected hyCont stop flag in loop header:\n{go}"
    );
    assert!(
        go.contains(" = i + 1, i < hyHi"),
        "expected post-statement evaluation of stop condition:\n{go}"
    );
    // Ensure the old premature check `hyCont = i != hyHi` is not injected into the body
    assert!(
        !go.contains("!= hyHi"),
        "premature body stop check must not be emitted:\n{go}"
    );
}

#[test]
fn test_inclusive_range_runaway_loopvar_reassign() {
    let src = r#"
fn main() {
    runs := 0
    for i in 1..=3 {
        runs += 1
        if i == 1 {
            i = 5
        }
    }
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains(" = i + 1, i < hyHi"),
        "expected post-statement stop flag check:\n{go}"
    );
    assert!(
        !go.contains("!= hyHi"),
        "body should not contain premature stop check:\n{go}"
    );
}

#[test]
fn test_inclusive_range_type_maximum_guard() {
    let src = r#"
fn main() {
    var high int8 = 127
    for value in high..=high {
        fmt.println(value)
    }
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains(" = value + 1, value < hyHi"),
        "expected post-statement type-max safe update:\n{go}"
    );
    assert!(
        !go.contains("for value := high; value <= high; value++"),
        "must not emit wrapping post-increment:\n{go}"
    );
}
