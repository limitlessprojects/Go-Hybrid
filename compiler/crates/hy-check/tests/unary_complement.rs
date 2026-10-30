use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(source: &str) -> hy_check::CheckResult {
    check_file(&parse_file("complement.hy", source).expect("valid unary-complement syntax"))
}

#[test]
fn integer_complements_are_accepted_in_expressions_and_constants() {
    for body in [
        "x := 3\nfmt.println(^x)",
        "const x = ^3\nfmt.println(x)",
        "var x u8 = ^u8(1)\nfmt.println(x)",
        "var x u16 = ^u16(0)\nfmt.println(x)",
        "var x i8 = ^i8(0)\nfmt.println(x)",
        "x := 3\nfmt.println(^^x, ^x ^ 2, ^(x ^ 2))",
        "var x u8 = 1\nfmt.println((^x) + 2)",
    ] {
        let result = check(&format!("fn main() {{ {body} }}"));
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "{body}: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn unsigned_constant_width_is_preserved_through_aliases_and_bounds() {
    for source in [
        "const x u8 = 254\nfn main() { var a [^x]int\nfmt.println(len(a)) }",
        "type octet = u8\nconst x octet = 254\nfn main() { var a [^x]int\nfmt.println(len(a)) }",
        "const x = u8(254)\nfn main() { var a [^x]int\nfmt.println(len(a)) }",
        "fn main() { const x u8 = 254\nvar a [^x]int\nfmt.println(len(a)) }",
        "fn main() { const x = u8(254)\nvar a [^x]int\nfmt.println(len(a)) }",
        "const x u8 = 254\nfn main() { const x = -2\nvar a [^x]int\nfmt.println(len(a)) }",
        "const x u8 = 0\nconst y = ^x\nfn main() { var n u8 = y\nfmt.println(n) }",
    ] {
        let result = check(source);
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "{source}: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn complements_reject_non_integer_operands() {
    for operand in ["true", "1.5", "\"text\"", "[]int{1}", "nil"] {
        let source = format!("fn main() {{ fmt.println(^{operand}) }}");
        let result = check(&source);
        assert_eq!(result.verdict, Verdict::Reject, "{source}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "HYB-E1002" && d.message.contains("operator `^`")),
            "{:?}",
            result.diagnostics
        );
    }
}

#[test]
fn complement_requires_exactly_one_returned_value() {
    for function in ["fn pair() (int, int) { return 1, 2 }", "fn pair() {}"] {
        let result = check(&format!("{function}\nfn main() {{ fmt.println(^pair()) }}"));
        assert_eq!(result.verdict, Verdict::Reject);
        assert!(result.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
    }
}

#[test]
fn named_unsigned_complement_retains_width_in_overflow_and_divisors() {
    for declaration in ["const mask u8 = 1", "type octet = u8\nconst mask octet = 1"] {
        let result = check(&format!(
            "{declaration}\nfn main() {{ fmt.println((^mask) + 2) }}"
        ));
        assert_eq!(result.verdict, Verdict::Reject);
        assert!(
            result.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
            "{:?}",
            result.diagnostics
        );
    }
    let zero = check("const mask u8 = 255\nfn main() { fmt.println(1 / ^mask) }");
    assert_eq!(zero.verdict, Verdict::Reject);
    assert!(
        zero.diagnostics.iter().any(|d| d.code == "HYB-E6001"),
        "{:?}",
        zero.diagnostics
    );
}

#[test]
fn typed_constant_complement_uses_unsigned_value_for_range_checks() {
    for expression in ["(^u8(1)) + 2", "(^u16(1)) + 2", "(^i8(0)) + 129"] {
        let result = check(&format!("fn main() {{ fmt.println({expression}) }}"));
        assert_eq!(result.verdict, Verdict::Reject, "{expression}");
        assert!(
            result.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
            "{:?}",
            result.diagnostics
        );
    }
}

#[test]
fn complement_constant_results_are_checked_in_array_and_map_keys() {
    let negative = check("fn main() { var a [^0]int\nfmt.println(a) }");
    assert_eq!(negative.verdict, Verdict::Reject);
    assert!(negative.diagnostics.iter().any(|d| d.code == "HYB-E6004"));

    let duplicate = check("fn main() { m := map[int]int{^0: 1, -1: 2}\nfmt.println(m) }");
    assert_eq!(duplicate.verdict, Verdict::Reject);
    assert!(duplicate.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
}

#[test]
fn out_of_range_literal_complements_reject_without_guessing_compound_values() {
    for operand in [
        "170141183460469231731687303715884105728",
        "0x80000000000000000000000000000000",
        "-170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105729",
    ] {
        let result = check(&format!(
            "fn main() {{ var x i8 = ^({operand})\nfmt.println(x) }}"
        ));
        assert_eq!(result.verdict, Verdict::Reject, "{operand}");
        assert!(
            result.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
            "{:?}",
            result.diagnostics
        );
    }

    for expression in [
        "^((1 << 200) - (1 << 200) - 1)",
        "^(-170141183460469231731687303715884105728) - 170141183460469231731687303715884105727",
        "^(-0x80000000000000000000000000000000) - 0x7fffffffffffffffffffffffffffffff",
    ] {
        let result = check(&format!(
            "fn main() {{ var x i8 = {expression}\nfmt.println(x) }}"
        ));
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "{expression}: {:?}",
            result.diagnostics
        );
    }
}
