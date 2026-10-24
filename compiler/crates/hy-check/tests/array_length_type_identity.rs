//! Regression test: an array's length is part of its type.
//!
//! `OpKind::Array` used to carry only the element kind, so `[2]int` and
//! `[3]int` were the same kind to every buildable check. Assigning, passing,
//! returning, storing, or comparing arrays of different lengths passed
//! `hyb check` with 0 diagnostics and failed `go build`
//! (`cannot use b (variable of type [3]int) as [2]int value ...`,
//! `invalid operation: a == b (mismatched types [2]int and [3]int)`).
//!
//! The kind now carries the length when it folds to a constant; an unknown
//! length on either side stays fail-open.
//!
//! Rule: `BUILDABLE` (GUARANTEES.md Row 13). Diagnostic: `HYB-E1002`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn rejects_array_length_mismatch_in_every_position() {
    // assignment
    assert_e1002(
        "fn main() {\n    var a [2]int\n    var b [3]int\n    a = b\n    fmt.println(a)\n}\n",
    );
    // variable initializer
    assert_e1002("fn main() {\n    var b [3]int\n    var a [2]int = b\n    fmt.println(a)\n}\n");
    // call argument
    assert_e1002(
        "fn take(a [2]int) int {\n    return a[0]\n}\nfn main() {\n    var b [3]int\n    fmt.println(take(b))\n}\n",
    );
    // return
    assert_e1002(
        "fn mk() [2]int {\n    var b [3]int\n    return b\n}\nfn main() {\n    fmt.println(mk())\n}\n",
    );
    // struct literal field
    assert_e1002(
        "struct s {\n    a [2]int\n}\nfn main() {\n    var b [3]int\n    x := s{a: b}\n    fmt.println(x)\n}\n",
    );
    // map element store
    assert_e1002(
        "fn main() {\n    m := map[string][2]int{}\n    var b [3]int\n    m[\"k\"] = b\n    fmt.println(m)\n}\n",
    );
    // equality operators
    assert_e1002(
        "fn main() {\n    var a [2]int\n    var b [3]int\n    fmt.println(a == b)\n}\n",
    );
    assert_e1002(
        "fn main() {\n    var a [2]int\n    var b [3]int\n    fmt.println(a != b)\n}\n",
    );
}

#[test]
fn rejects_length_mismatch_through_literals_aliases_and_nesting() {
    // `[...]T` literal: the length is the item count
    assert_e1002(
        "fn main() {\n    a := [...]int{1, 2}\n    var b [3]int = a\n    fmt.println(b)\n}\n",
    );
    // type alias resolves to its array length
    assert_e1002(
        "type pair = [2]int\nfn main() {\n    var b [3]int\n    var p pair = b\n    fmt.println(p)\n}\n",
    );
    // the inner length of a nested array
    assert_e1002(
        "fn main() {\n    grid := [2][3]int{}\n    var row [2]int = grid[0]\n    fmt.println(row)\n}\n",
    );
}

#[test]
fn accepts_matching_array_lengths() {
    // const-expression length, `[...]` literal, and explicit literal all agree on 3
    assert_ok(
        "const n = 3\nfn take(a [3]int) int {\n    return a[0]\n}\nfn main() {\n    var b [n]int\n    c := [...]int{1, 2, 3}\n    d := [3]int{4, 5, 6}\n    fmt.println(take(b), take(c), take(d), b == c, c != d)\n    var e [n + 0]int = d\n    e = c\n    fmt.println(e)\n}\n",
    );
    assert_ok(
        "type trio = [3]int\nfn mk() trio {\n    return [3]int{1, 2, 3}\n}\nfn main() {\n    var x [3]int = mk()\n    fmt.println(x == mk())\n}\n",
    );
    assert_ok(
        "fn main() {\n    grid := [2][3]int{}\n    var row [3]int = grid[0]\n    fmt.println(row)\n}\n",
    );
}
