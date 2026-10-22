use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn default_identifier_mangled_in_all_positions_issue_1167() {
    let src = r#"
struct default {
    default int
}

fn helper(default int) int {
    return default + 1
}

fn main() {
    default := make(chan int, 1)
    default <- 7
    v := <-default
    s := default{default: helper(v)}
    select {
        default <- s.default:
            fmt.println("sent")
        default:
            fmt.println("{s.default}")
    }
}
"#;
    let go = emit(src);
    assert!(
        go.contains("type hyKw_default struct"),
        "expected struct default to mangle to hyKw_default, got:\n{go}"
    );
    assert!(
        go.contains("func helper(hyKw_default int) int"),
        "expected param default to mangle to hyKw_default, got:\n{go}"
    );
    assert!(
        go.contains("hyKw_default := make(chan int, 1)"),
        "expected local default channel to mangle to hyKw_default, got:\n{go}"
    );
    assert!(
        go.contains("default:"),
        "expected select default: arm head to remain default:, got:\n{go}"
    );
}
