//! Regression tests for #3639 (GUARANTEES Row 13 BUILDABLE):
//! `time.nanoseconds(...)` and `time.microseconds(...)` must lower to
//! `time.Duration(...) * time.Nanosecond` and `time.Duration(...) * time.Microsecond`,
//! matching `time.milliseconds`, `time.seconds`, `time.minutes`, and `time.hours`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("must parse");
    emit_go(&ast).expect("must emit")
}

#[test]
fn test_time_nanoseconds_lowering() {
    let src = r#"
import "time"

fn main() {
    d := time.nanoseconds(500)
    fmt.println(d)
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("time.Duration(500) * time.Nanosecond"),
        "expected time.Duration(500) * time.Nanosecond, got:\n{go}"
    );
    assert!(
        !go.contains("time.Nanoseconds"),
        "must not emit nonexistent time.Nanoseconds, got:\n{go}"
    );
}

#[test]
fn test_time_microseconds_lowering() {
    let src = r#"
import "time"

fn main() {
    d := time.microseconds(25)
    fmt.println(d)
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("time.Duration(25) * time.Microsecond"),
        "expected time.Duration(25) * time.Microsecond, got:\n{go}"
    );
    assert!(
        !go.contains("time.Microseconds"),
        "must not emit nonexistent time.Microseconds, got:\n{go}"
    );
}

#[test]
fn test_time_duration_with_variables() {
    let src = r#"
import "time"

fn delay(n int) {
    ns := time.nanoseconds(n)
    us := time.microseconds(n)
    fmt.println(ns, us)
}

fn main() {
    delay(100)
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("time.Duration(n) * time.Nanosecond"),
        "expected time.Duration(n) * time.Nanosecond, got:\n{go}"
    );
    assert!(
        go.contains("time.Duration(n) * time.Microsecond"),
        "expected time.Duration(n) * time.Microsecond, got:\n{go}"
    );
}

#[test]
fn test_all_duration_units_consistency() {
    let src = r#"
import "time"

fn main() {
    d1 := time.nanoseconds(1)
    d2 := time.microseconds(2)
    d3 := time.milliseconds(3)
    d4 := time.seconds(4)
    d5 := time.minutes(5)
    d6 := time.hours(6)
    fmt.println(d1, d2, d3, d4, d5, d6)
}
"#;
    let go = emit_src(src);
    assert!(go.contains("time.Duration(1) * time.Nanosecond"));
    assert!(go.contains("time.Duration(2) * time.Microsecond"));
    assert!(go.contains("time.Duration(3) * time.Millisecond"));
    assert!(go.contains("time.Duration(4) * time.Second"));
    assert!(go.contains("time.Duration(5) * time.Minute"));
    assert!(go.contains("time.Duration(6) * time.Hour"));
}
