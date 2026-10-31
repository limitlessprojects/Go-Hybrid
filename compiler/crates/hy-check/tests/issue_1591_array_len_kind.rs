//! issue_1591_array_len_kind.rs — regression test for issue #1591:
//! array length is erased from the kind model: [2]int and [3]int assign, pass,
//! return and compare freely, fail go build.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_1591.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn mismatched_array_length_assignment_and_init_rejected_e1002() {
    let cases = [
        // Assignment
        "fn main() {\n    var a [2]int\n    var b [3]int\n    a = b\n    fmt.println(a)\n}\n",
        // Variable init
        "fn main() {\n    var b [3]int\n    var a [2]int = b\n    fmt.println(a)\n}\n",
        // Call argument
        "fn take(a [2]int) {}\nfn main() {\n    var b [3]int\n    take(b)\n}\n",
        // Return statement
        "fn mk(b [3]int) [2]int {\n    return b\n}\nfn main() {}\n",
        // Struct literal
        "struct sample_box {\n    a [2]int\n}\nfn main() {\n    var b [3]int\n    s := sample_box{a: b}\n    fmt.println(s)\n}\n",
        // Equality comparison
        "fn main() {\n    var a [2]int\n    var b [3]int\n    if a == b {\n        fmt.println(\"eq\")\n    }\n}\n",
        // Inequality comparison
        "fn main() {\n    var a [2]int\n    var b [3]int\n    if a != b {\n        fmt.println(\"ne\")\n    }\n}\n",
        // [...] literal vs declared array
        "fn main() {\n    a := [...]int{1, 2}\n    var b [3]int = a\n    fmt.println(b)\n}\n",
        // Type alias length mismatch
        "type pair = [2]int\nfn main() {\n    var b [3]int\n    var p pair = b\n    fmt.println(p)\n}\n",
        // Nested array index vs target
        "fn main() {\n    grid := [2][3]int{}\n    var row [2]int = grid[0]\n    fmt.println(row)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for mismatched array length:\n{src}\ngot diagnostics: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn matching_array_length_controls_accepted() {
    let cases = [
        "fn main() {\n    var a [2]int\n    var b [2]int\n    a = b\n    fmt.println(a)\n}\n",
        "fn main() {\n    var a [2]int = [2]int{1, 2}\n    fmt.println(a)\n}\n",
        "fn main() {\n    a := [...]int{1, 2}\n    var b [2]int = a\n    fmt.println(b)\n}\n",
        "type pair = [2]int\nfn main() {\n    var b [2]int\n    var p pair = b\n    fmt.println(p)\n}\n",
        "fn main() {\n    grid := [2][3]int{}\n    var row [3]int = grid[0]\n    fmt.println(row)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            !cs.iter().any(|c| c == "HYB-E1002"),
            "matching array length must NOT fire HYB-E1002:\n{src}\ngot diagnostics: {:?}",
            r.diagnostics
        );
    }
}
