use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn inclusive_range_post_step_updates_stop_flag_on_loopvar_write() {
    let go = emit_src(
        "\
package main

fn main() {
    xs := [10, 20, 30, 40]
    last := len(xs) - 1
    for i in 0..=last {
        if xs[i] == 30 {
            i += 1
        }
    }
}
",
    );

    assert!(
        go.contains(", i, hyCont"),
        "expected post statement to step both loop variable and stop flag, got:\n{go}"
    );
    assert!(
        !go.contains("hyCont0 = i != hyHi"),
        "body should not inject hyCont assignment at start of body, got:\n{go}"
    );
}
