//! Regression tests for zero-field structs (`struct unit {}`, `struct unit[t any] {}`)
//! and user-declared `set`/`option` struct instantiation arity:
//!
//! 1. `check_struct_literal` (`buildable.rs`) returned early when `structs.get(&sname)`
//!    was `None`, allowing unknown and duplicate fields in zero-field struct literals
//!    (`unit{bogus: 1}`, `unit[int]{bogus: 1}`, `unit{a: 1, a: 2}`) to bypass `HYB-E1002`.
//! 2. `check_named_type_instantiation` (`buildable.rs`) unconditionally short-circuited
//!    on `head == "set"` and `head == "option"`, bypassing generic type-argument arity
//!    checks (`HYB-E1002`) when a user declared their own `struct set` or `struct option`.
//! 3. `range_iter.rs`, `emit_guard.rs`, and `shape.rs` only checked
//!    `!structs.contains_key("set")` / `!structs.contains_key(name)`, treating a
//!    user-declared zero-field `struct set[t any] {}` as the T9 prelude `set[T]` in
//!    `for x in s` (`HYB-E2009`) and whole-struct equality (`s1 == s2`).

use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("source should parse");
    check_file(&ast)
}

#[test]
fn zero_field_struct_literal_rejects_unknown_and_duplicate_fields_e1002() {
    let cases = [
        (
            "non-generic zero-field struct unknown field",
            "struct unit {}\n\
             fn main() {\n\
                 u := unit{bogus: 1}\n\
                 _ = u\n\
             }\n",
            "unknown field `bogus` in `unit` literal",
        ),
        (
            "generic zero-field struct unknown field",
            "struct unit[t any] {}\n\
             fn main() {\n\
                 u := unit[int]{bogus: 1}\n\
                 _ = u\n\
             }\n",
            "unknown field `bogus` in `unit[int]` literal",
        ),
        (
            "type alias to zero-field struct unknown field",
            "struct unit {}\n\
             type my_unit = unit\n\
             fn main() {\n\
                 u := my_unit{bogus: 1}\n\
                 _ = u\n\
             }\n",
            "unknown field `bogus` in `unit` literal",
        ),
        (
            "zero-field struct duplicate field",
            "struct unit {}\n\
             fn main() {\n\
                 u := unit{a: 1, a: 2}\n\
                 _ = u\n\
             }\n",
            "duplicate field `a` in `unit` literal",
        ),
    ];

    for (label, src, expected_msg) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case `{label}` must be rejected, got: {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E1002" && d.message.contains(expected_msg)),
            "case `{label}` expected HYB-E1002 containing `{expected_msg}`, got: {:?}",
            r.diagnostics
        );
    }

    // Positive control: empty zero-field struct literals accept.
    let ok = check(
        "struct unit {}\n\
         struct marker[t any] {}\n\
         type my_unit = unit\n\
         fn main() {\n\
             u := unit{}\n\
             m := marker[int]{}\n\
             a := my_unit{}\n\
             _ = u\n\
             _ = m\n\
             _ = a\n\
         }\n",
    );
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
}

#[test]
fn user_declared_set_and_option_struct_instantiation_arity_rejects_e1002() {
    let cases = [
        (
            "non-generic struct set instantiated with type args in new",
            "struct set { x int }\n\
             fn main() {\n\
                 p := new(set[int])\n\
                 _ = p\n\
             }\n",
            "`set` is not a generic type",
        ),
        (
            "zero-field non-generic struct set instantiated with type args",
            "struct set {}\n\
             fn main() {\n\
                 p := new(set[int])\n\
                 _ = p\n\
             }\n",
            "`set` is not a generic type",
        ),
        (
            "generic struct set instantiated with wrong type arg count",
            "struct set[t any] {}\n\
             fn main() {\n\
                 var s set[int, string]\n\
                 _ = s\n\
             }\n",
            "wrong number of type arguments for `set`",
        ),
        (
            "generic struct set used without type args in new",
            "struct set[t any] {}\n\
             fn main() {\n\
                 p := new(set)\n\
                 _ = p\n\
             }\n",
            "cannot use generic struct `set` without type arguments",
        ),
        (
            "non-generic struct option instantiated with 1 type arg",
            "struct option {}\n\
             fn main() {\n\
                 var o option[int]\n\
                 _ = o\n\
             }\n",
            "`option` is not a generic type",
        ),
        (
            "2-param struct option instantiated with 1 type arg",
            "struct option[k any, v any] {}\n\
             fn main() {\n\
                 var o option[int]\n\
                 _ = o\n\
             }\n",
            "wrong number of type arguments for `option`",
        ),
        (
            "generic struct option used without type args",
            "struct option[t any] {}\n\
             fn main() {\n\
                 var o option\n\
                 _ = o\n\
             }\n",
            "cannot use generic struct `option` without type arguments",
        ),
    ];

    for (label, src, expected_msg) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case `{label}` must be rejected, got: {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E1002" && d.message.contains(expected_msg)),
            "case `{label}` expected HYB-E1002 containing `{expected_msg}`, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn user_declared_zero_field_set_and_primitive_shadowing() {
    // A user-declared zero-field `struct set[t any] {}` shadows the T9 prelude `set`,
    // so ranging over `s` must be rejected with HYB-E2009 (`not iterable`).
    let bad_range = check(
        "struct set[t any] {}\n\
         fn main() {\n\
             s := set[int]{}\n\
             for x in s {\n\
                 _ = x\n\
             }\n\
         }\n",
    );
    assert_eq!(bad_range.verdict, Verdict::Reject, "{:?}", bad_range.diagnostics);
    assert!(
        bad_range.diagnostics.iter().any(|d| d.code == "HYB-E2009"),
        "{:?}",
        bad_range.diagnostics
    );

    // Positive control: comparing two user-declared zero-field `set[bool]` structs or
    // declaring a method on a user-declared zero-field `struct int {}` is valid Go.
    let ok = check(
        "struct set[t any] {}\n\
         struct int {}\n\
         fn (x int) ok() bool {\n\
             return true\n\
         }\n\
         fn main() {\n\
             a := set[bool]{}\n\
             b := set[bool]{}\n\
             v := int{}\n\
             if a == b && v.ok() {\n\
             }\n\
         }\n",
    );
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
}
