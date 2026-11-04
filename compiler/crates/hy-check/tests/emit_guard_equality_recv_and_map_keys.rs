//! Soundness regression tests for `emit_guard.rs`:
//! 1. `HYB-E2006`: `==` / `!=` on generic top-level function instantiations,
//!    file-level `const` function aliases, struct method values, associated
//!    function values, and `move`-wrapped incomparable values.
//! 2. `HYB-E2007`: invalid method receivers on instantiated/generic type
//!    aliases, foreign package types (`pkg.type`), and prelude `set[T]`.
//! 3. `HYB-E1002`: duplicate constant keys in map literals via fixed-width
//!    integer truncation (`u8(300)` == `44`, `u8(-1)` == `255`), type-alias
//!    conversions, float conversions, and `len(...)` over string/array constants.
//! 4. `HYB-E6002`: local variables shadowing an overflowing file-level `const`
//!    in call arguments and map literal keys/values.

use hy_check::{check_source, Verdict};

fn assert_rejects_with(src: &str, code: &str) {
    let report = check_source("test.hy", src).expect("parse ok");
    assert_eq!(
        report.verdict,
        Verdict::Reject,
        "expected Reject with {code}, got Accept:\n{src}"
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "expected {code}, got: {:#?}",
        report.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let report = check_source("test.hy", src).expect("parse ok");
    assert_eq!(
        report.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:#?}",
        report.diagnostics
    );
}

#[test]
fn equality_on_generic_fn_and_const_fn_alias_rejected_e2006() {
    assert_rejects_with(
        "fn id[t any](x t) t {\n    return x\n}\nfn main() {\n    _ = id[int] == id[int]\n}\n",
        "HYB-E2006",
    );
    assert_rejects_with(
        "fn helper() int {\n    return 1\n}\nconst alias_fn = helper\nfn main() {\n    _ = alias_fn == helper\n}\n",
        "HYB-E2006",
    );
}

#[test]
fn equality_on_method_value_and_assoc_fn_value_rejected_e2006() {
    assert_rejects_with(
        "struct counter {\n    n int\n}\nfn (c counter) get() int {\n    return c.n\n}\nfn main() {\n    c := counter{n: 1}\n    _ = c.get == c.get\n}\n",
        "HYB-E2006",
    );
    assert_rejects_with(
        "struct counter {\n    n int\n}\nfn counter.make(n int) counter {\n    return counter{n: n}\n}\nfn main() {\n    _ = counter.make == counter.make\n}\n",
        "HYB-E2006",
    );
}

#[test]
fn equality_on_move_wrapped_slice_and_fn_rejected_e2006() {
    assert_rejects_with(
        "fn main() {\n    a := []int{1, 2}\n    b := []int{1, 2}\n    _ = (move a) == (move b)\n}\n",
        "HYB-E2006",
    );
}

#[test]
fn method_receiver_on_generic_type_alias_and_foreign_type_rejected_e2007() {
    assert_rejects_with(
        "struct holder[t any] {\n    v t\n}\ntype alias_holder = holder[int]\nfn (h alias_holder[int]) val() int {\n    return 0\n}\nfn main() {}\n",
        "HYB-E2007",
    );
    assert_rejects_with(
        "import \"sync\"\nfn (m sync.mutex) bad() {}\nfn main() {}\n",
        "HYB-E2007",
    );
    assert_rejects_with(
        "fn (s set[int]) extra() int {\n    return 0\n}\nfn main() {}\n",
        "HYB-E2007",
    );
}

#[test]
fn map_literal_duplicate_keys_via_truncation_alias_len_and_float_rejected_e1002() {
    assert_rejects_with(
        "fn main() {\n    m := map[u8]int{44: 1, u8(300): 2}\n    _ = m\n}\n",
        "HYB-E1002",
    );
    assert_rejects_with(
        "fn main() {\n    m := map[u8]int{255: 1, u8(-1): 2}\n    _ = m\n}\n",
        "HYB-E1002",
    );
    assert_rejects_with(
        "type key_id = int\nfn main() {\n    m := map[key_id]int{1: 10, key_id(1): 20}\n    _ = m\n}\n",
        "HYB-E1002",
    );
    assert_rejects_with(
        "fn main() {\n    m := map[int]int{3: 10, len(\"abc\"): 20}\n    _ = m\n}\n",
        "HYB-E1002",
    );
    assert_rejects_with(
        "fn main() {\n    m := map[f64]int{1.5: 10, f64(1.5): 20}\n    _ = m\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn shadowed_file_const_in_call_and_map_literal_accepted_while_unshadowed_rejected_e6002() {
    assert_accepts(
        "const big = 300\nfn take_u8(v u8) {\n    _ = v\n}\nfn main() {\n    big := u8(10)\n    take_u8(big)\n    m := map[u8]u8{big: big}\n    _ = m\n}\n",
    );
    assert_rejects_with(
        "const big = 300\nfn take_u8(v u8) {\n    _ = v\n}\nfn main() {\n    take_u8(big)\n}\n",
        "HYB-E6002",
    );
}

#[test]
fn distinct_map_keys_and_valid_field_equality_accepted() {
    assert_accepts(
        "struct counter {\n    n int\n}\nfn (c counter) get() int {\n    return c.n\n}\ntype key_id = int\nfn main() {\n    c1 := counter{n: 1}\n    c2 := counter{n: 1}\n    _ = c1.n == c2.n\n    _ = c1.get() == c2.get()\n    m1 := map[u8]int{44: 1, u8(301): 2}\n    m2 := map[key_id]int{1: 10, key_id(2): 20}\n    m3 := map[int]int{3: 10, len(\"abcd\"): 20}\n    _ = m1\n    _ = m2\n    _ = m3\n}\n",
    );
}
