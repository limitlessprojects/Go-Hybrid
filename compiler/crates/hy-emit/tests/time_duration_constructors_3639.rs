use hy_emit::emit_file;
use hy_syntax::parse_file;

#[test]
fn test_time_nanoseconds_and_microseconds_lower_to_duration_3639() {
    let src = r#"
import "time"

fn main() {
    d1 := time.nanoseconds(500)
    d2 := time.microseconds(25)
    _ = d1
    _ = d2
}
"#;
    let file = parse_file("repro.hy", src).expect("parse failed");
    let emitted = emit_file(&file).expect("emit failed");
    assert!(
        emitted.contains("time.Duration(500) * time.Nanosecond"),
        "expected time.Duration(500) * time.Nanosecond, got:
{}",
        emitted
    );
    assert!(
        emitted.contains("time.Duration(25) * time.Microsecond"),
        "expected time.Duration(25) * time.Microsecond, got:
{}",
        emitted
    );
}
