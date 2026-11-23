//! Regression tests for issue #4618:
//! [soundness] hy-emit: interpolated printf formats are pre-rendered with fmt.Sprintf,
//! so '%' in a value becomes a verb, '%w' wrapping breaks, and an interpolated value
//! argument is spliced as the format.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn test_fmt_printf_with_extra_args() {
    let src = r#"
import "fmt"

fn main() {
    name := "100%d"
    n := 5
    fmt.printf("{name} n=%d\n", n)
}
"#;
    let go = emit(src);
    assert!(
        go.contains(r#"fmt.Printf("%v n=%d\n", name, n)"#),
        "expected spliced fmt.Printf with extra arguments, got:\n{go}"
    );
    assert!(
        !go.contains("fmt.Sprintf"),
        "did not expect intermediate fmt.Sprintf wrap, got:\n{go}"
    );
}

#[test]
fn test_fmt_sprintf_interp() {
    let src = r#"
import "fmt"

fn main() {
    p := "50%"
    fmt.println(fmt.sprintf("rate: {p}"))
}
"#;
    let go = emit(src);
    assert!(
        go.contains(r#"fmt.Sprintf("rate: %v", p)"#),
        "expected single fmt.Sprintf splice, got:\n{go}"
    );
    assert!(
        !go.contains(r#"fmt.Sprintf(fmt.Sprintf"#),
        "did not expect nested double fmt.Sprintf, got:\n{go}"
    );
}

#[test]
fn test_fmt_errorf_preserves_error_wrapping() {
    let src = r#"
import "errors"
import "fmt"

fn main() {
    base := errors.new("base")
    p := "50%"
    e := fmt.errorf("ctx {p}: %w", base)
    _ = e
}
"#;
    let go = emit(src);
    assert!(
        go.contains(r#"fmt.Errorf("ctx %v: %w", p, base)"#),
        "expected fmt.Errorf with %w error wrapping preserved, got:\n{go}"
    );
}

#[test]
fn test_log_printf_interp() {
    let src = r#"
import "log"

fn main() {
    p := "50%"
    log.printf("pkg {p}")
}
"#;
    let go = emit(src);
    assert!(
        go.contains(r#"log.Printf("pkg %v", p)"#),
        "expected log.Printf spliced interp, got:\n{go}"
    );
}

#[test]
fn test_interp_in_value_position_not_spliced_as_format() {
    let src = r#"
import "fmt"

fn main() {
    a := 3
    fmt.printf("[%s]\n", "{a}")
}
"#;
    let go = emit(src);
    assert!(
        go.contains(r#"fmt.Printf("[%s]\n", fmt.Sprintf("%v", a))"#),
        "expected format string to remain [%s]\\n and value to be Sprintf, got:\n{go}"
    );
}
