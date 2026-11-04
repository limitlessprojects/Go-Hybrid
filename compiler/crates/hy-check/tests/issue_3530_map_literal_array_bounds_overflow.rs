use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("t.hy", src).expect("parses"))
}

#[test]
fn map_literal_array_value_exceeding_length_fails_closed_3530() {
    let cases = [
        "fn main() {\n  x := map[string][2]int{\"a\": {1, 2, 3}}\n  _ = x\n}",
        "fn main() {\n  x := map[string][2][2]int{\"a\": {{1, 2}, {3, 4, 5}}}\n  _ = x\n}",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for: {src}");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E6004" && d.message.contains("exceeding array length")),
            "expected HYB-E6004 exceeding array length for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn map_literal_array_key_exceeding_length_fails_closed_3530() {
    let src = "fn main() {\n  x := map[[2]int]string{{1, 2, 3}: \"a\"}\n  _ = x\n}";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E6004" && d.message.contains("exceeding array length")),
        "expected HYB-E6004 exceeding array length, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn map_literal_element_overflow_fails_closed_3530() {
    let cases = [
        "fn main() {\n  x := map[string][2]uint8{\"a\": {300, 1}}\n  _ = x\n}",
        "fn main() {\n  x := map[string][]uint8{\"a\": {300, 1}}\n  _ = x\n}",
        "fn main() {\n  x := map[[2]uint8]string{{300, 1}: \"a\"}\n  _ = x\n}",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for: {src}");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E6002" && d.message.contains("overflows")),
            "expected HYB-E6002 element overflow for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn valid_map_literals_remain_accepted_3530() {
    let valid = [
        "fn main() {\n  x := map[string][2]int{\"a\": {1, 2}}\n  _ = x\n}",
        "fn main() {\n  y := map[[2]int]string{{1, 2}: \"a\"}\n  _ = y\n}",
        "fn main() {\n  z := map[string][]uint8{\"a\": {100, 200}}\n  _ = z\n}",
        "fn main() {\n  w := map[string][2][2]int{\"a\": {{1, 2}, {3, 4}}}\n  _ = w\n}",
    ];
    for src in valid {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for valid syntax: {src}, got errors: {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics.is_empty(),
            "expected 0 diagnostics for valid syntax: {src}, got: {:?}",
            r.diagnostics
        );
    }
}
