//! Binding-shape guards (T23 source counts) — #929 class.
//!
//! A channel receive and a map index yield at most two values (`v` or
//! `v, ok`); indexing a slice, array, or string yields exactly one and has
//! no comma-ok form. Binding a count the source cannot yield — including a
//! 1-result call bound to 2+ names, and a select receive arm with 3+ names
//! (which crashes the emitter's select lowering) — fails go build with
//! `assignment mismatch` and is rejected fail-closed before emission. Every
//! valid 1-value and comma-ok 2-value form stays legal.

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

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn one_result_call_to_multiple_names_reject() {
    assert_code(
        "fn one() int {\n    return 1\n}\nfn main() {\n    a, b := one()\n    _ = a\n    _ = b\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn one() int {\n    return 1\n}\nfn main() {\n    var a int\n    var b int\n    a, b = one()\n    _ = a\n    _ = b\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn receive_over_two_names_reject() {
    assert_code(
        "fn main() {\n    ch := make(chan int, 1)\n    a, b, c := <-ch\n    _ = a\n    _ = b\n    _ = c\n}\n",
        "HYB-E2002",
    );
    assert_code(
        "fn main() {\n    ch := make(chan int, 1)\n    var a int\n    var b int\n    var c int\n    a, b, c = <-ch\n    _ = a\n    _ = b\n    _ = c\n}\n",
        "HYB-E2002",
    );
}

#[test]
fn map_index_over_two_names_reject() {
    assert_code(
        "fn main() {\n    m := map[string]int{\"k\": 1}\n    a, b, c := m[\"k\"]\n    _ = a\n    _ = b\n    _ = c\n}\n",
        "HYB-E2002",
    );
}

#[test]
fn slice_index_multi_name_reject() {
    assert_code(
        "fn main() {\n    xs := []int{1, 2}\n    a, b := xs[0]\n    _ = a\n    _ = b\n}\n",
        "HYB-E2002",
    );
    assert_code(
        "fn main() {\n    xs := []int{1, 2}\n    var a int\n    var b int\n    a, b = xs[0]\n    _ = a\n    _ = b\n}\n",
        "HYB-E2002",
    );
}

#[test]
fn select_recv_three_names_reject() {
    assert_code(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n    a, b, c := <-ch:\n        fmt.println(a)\n    }\n}\n",
        "HYB-E2002",
    );
}

#[test]
fn valid_source_forms_stay_legal() {
    // 1-value and comma-ok 2-value forms (issue accept controls)
    assert_ok(
        "fn main() {\n    ch := make(chan int, 1)\n    v := <-ch\n    _ = v\n}\n",
    );
    assert_ok(
        "fn main() {\n    ch := make(chan int, 1)\n    v, ok := <-ch\n    _ = v\n    _ = ok\n}\n",
    );
    assert_ok(
        "fn main() {\n    m := map[string]int{\"k\": 1}\n    v := m[\"k\"]\n    _ = v\n}\n",
    );
    assert_ok(
        "fn main() {\n    m := map[string]int{\"k\": 1}\n    v, ok := m[\"k\"]\n    _ = v\n    _ = ok\n}\n",
    );
    assert_ok(
        "fn main() {\n    xs := []int{1, 2}\n    v := xs[0]\n    _ = v\n}\n",
    );
    assert_ok(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n    v, ok := <-ch:\n        fmt.println(v, ok)\n    }\n}\n",
    );
    assert_ok(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n    v := <-ch:\n        fmt.println(v)\n    }\n}\n",
    );
    // 2-result call to 2 names stays legal (the T23 baseline)
    assert_ok(
        "fn minmax(a int, b int) (int, int) {\n    if a > b {\n        return a, b\n    }\n    return b, a\n}\nfn main() {\n    lo, hi := minmax(3, 9)\n    _ = lo\n    _ = hi\n}\n",
    );
    // 1-result call to 1 name stays legal
    assert_ok(
        "fn one() int {\n    return 1\n}\nfn main() {\n    x := one()\n    _ = x\n}\n",
    );
}
