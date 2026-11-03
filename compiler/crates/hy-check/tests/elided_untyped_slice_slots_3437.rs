//! Issue #3437 (Row 13 BUILDABLE): nested untyped slice literals inherit
//! context from a container slot, so the checker must validate their items
//! against concrete array/slice slots or infer them for interface slots.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn nested_untyped_literals_are_checked_in_array_slice_and_interface_slots() {
    for (name, src) in [
        (
            "map slice value rejects string element",
            "fn main() {\n m := map[string][]int{\"a\": [1, \"bad\"]}\n _ = m\n }",
        ),
        (
            "map slice value rejects float element",
            "fn main() {\n m := map[string][]int{\"a\": [1, 2.5]}\n _ = m\n }",
        ),
        (
            "map slice value rejects mismatched typed element",
            "fn main() {\n var x i64 = 2\n m := map[string][]int{\"a\": [1, x]}\n _ = m\n }",
        ),
        (
            "array of slices checks nested slice elements",
            "fn main() {\n xs := [1][]int{[1, \"bad\"]}\n _ = xs\n }",
        ),
        (
            "slice interface slot rejects uninferable head",
            "fn main() {\n var x int = 5\n xs := []any{[x]}\n _ = xs\n }",
        ),
        (
            "array interface slot checks inferred item types",
            "fn main() {\n xs := [1]any{[1, \"bad\"]}\n _ = xs\n }",
        ),
        (
            "map interface slot rejects uninferable head",
            "fn main() {\n var x int = 5\n m := map[string]any{\"a\": [x]}\n _ = m\n }",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}: {src}");
        assert!(
            errors(&r).contains(&"HYB-E1002".to_string()),
            "{name}: expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn elided_array_literals_are_accepted_in_map_array_slots() {
    for (name, src) in [
        (
            "map array value",
            "fn main() {\n m := map[string][2]int{\"a\": [1, 2]}\n _ = m\n }",
        ),
        (
            "map array key",
            "fn main() {\n m := map[[2]int]string{[1, 2]: \"ok\"}\n _ = m\n }",
        ),
    ] {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "{name}: expected Accept, got {:?}",
            r.diagnostics
        );
        assert!(
            errors(&r).is_empty(),
            "{name}: unexpected errors {:?}",
            r.diagnostics
        );
    }
}
