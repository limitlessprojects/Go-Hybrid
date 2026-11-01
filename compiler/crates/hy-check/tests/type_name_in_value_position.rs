//! #778 — a type name used where a VALUE is required.
//!
//! `struct holder { n int }` followed by `x := holder` passes `hyb check` and
//! the emitter prints the identifier verbatim, so `go build` fails with
//! `holder (type) is not an expression`. Nothing runs, so there is no race or
//! ownership angle: this is the claim-precision class (GUARANTEES Row 13,
//! BUILDABLE), the same family as the #682 wave.
//!
//! The rule has to be NARROW, because a type name is a legal operand in a
//! conversion. Three positions are type positions, not value positions, and
//! the accept half of this file pins all three:
//!
//!   * the CALLEE of a call — `holder(1)` is a struct conversion,
//!     `int(3.7)` a built-in one;
//!   * the FIRST argument of `new`/`make` — `new(bag)`,
//!     `make(map[string]int)`;
//!   * anywhere a local binding shadows the type name — `holder := 5`.
//!
//! The `new(bag)` case is not hypothetical: it broke the corpus fixture
//! `n5-zero-map-first-write.hy`, which is exactly what the corpus is for.

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

const HOLDER: &str = "struct holder {\n    n int\n}\n\n";

// ---------------------------------------------------------------------------
// The four reported positions, plus a built-in type.
// ---------------------------------------------------------------------------

#[test]
fn type_name_assigned_to_a_variable_is_rejected() {
    assert_code(
        &format!("{HOLDER}fn main() {{\n    x := holder\n    fmt.println(\"{{x}}\")\n}}\n"),
        "HYB-E1002",
    );
}

#[test]
fn type_name_as_a_call_argument_is_rejected() {
    assert_code(
        &format!(
            "{HOLDER}fn take(h holder) int {{\n    return h.n\n}}\n\nfn main() {{\n    v := take(holder)\n    fmt.println(\"{{v}}\")\n}}\n"
        ),
        "HYB-E1002",
    );
}

#[test]
fn type_name_in_arithmetic_is_rejected() {
    assert_code(
        &format!("{HOLDER}fn main() {{\n    y := holder + 1\n    fmt.println(\"{{y}}\")\n}}\n"),
        "HYB-E1002",
    );
}

#[test]
fn type_name_as_a_return_value_is_rejected() {
    assert_code(
        &format!(
            "{HOLDER}fn make_it() holder {{\n    return holder\n}}\n\nfn main() {{\n    x := make_it()\n    fmt.println(\"{{x.n}}\")\n}}\n"
        ),
        "HYB-E1002",
    );
}

#[test]
fn builtin_type_name_in_a_value_position_is_rejected() {
    assert_code(
        "fn main() {\n    x := int\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn type_alias_name_in_a_value_position_is_rejected() {
    assert_code(
        "type tally = map[string]int\n\nfn main() {\n    x := tally\n    fmt.println(\"{len(x)}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn enum_name_in_a_value_position_is_rejected() {
    assert_code(
        "enum colour {\n    red\n    green\n}\n\nfn main() {\n    x := colour\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// #2199 — the same name in STORE-target position.
//
// `n string = "str"` parses as the expression statement `n` followed by a
// store to `string`, so the target of that second statement is the type name.
// The emitter prints it verbatim and nothing runs, exactly like the #778
// positions above.
// ---------------------------------------------------------------------------

#[test]
fn type_name_as_a_store_target_is_rejected() {
    assert_code(
        "fn main() {\n    var n int\n    n string = \"str\"\n    fmt.println(\"{n}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn type_name_store_target_family_is_rejected() {
    // The three reported rows: matching type, wrong type, and a struct field
    // store. The wrong-type one is the interesting one — `q.n = "str"` is
    // already HYB-E1002, and the annotation must not silence it quietly.
    assert_code(
        "fn main() {\n    var n int\n    n int = 5\n    fmt.println(\"{n}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        &format!(
            "{HOLDER}fn main() {{\n    var q holder\n    q.n int = 5\n    fmt.println(\"{{q.n}}\")\n}}\n"
        ),
        "HYB-E1002",
    );
}

#[test]
fn plain_stores_and_declarations_stay_accepted() {
    // The accept half: the annotation is legal in a declaration, and a plain
    // store has a real binding as its target.
    assert_ok("fn main() {\n    var n int = 5\n    fmt.println(\"{n}\")\n}\n");
    assert_ok("fn main() {\n    n := 5\n    fmt.println(\"{n}\")\n}\n");
    assert_ok("fn main() {\n    var n int\n    n = 5\n    fmt.println(\"{n}\")\n}\n");
    assert_ok(&format!(
        "{HOLDER}fn main() {{\n    h := holder{{n: 1}}\n    h.n = 2\n    fmt.println(\"{{h.n}}\")\n}}\n"
    ));
}

// ---------------------------------------------------------------------------
// The accept half: three TYPE positions a type name is legal in, plus the
// shadowing case. Each was verified to build.
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// The accept half: three TYPE positions a type name is legal in, plus the
// shadowing case. Each was verified to build.
// ---------------------------------------------------------------------------

#[test]
fn builtin_conversion_callee_is_not_a_value_position() {
    // The callee of a call is a call position. A built-in conversion puts a
    // type name there legally, so the check must not fire.
    //
    // `holder(1)` (a struct conversion with a positional argument) is NOT
    // asserted here: it is accepted but fails `go build` with `cannot convert
    // 1 (untyped int constant) to type holder` — a pre-existing, separate
    // defect. Asserting it as OK would encode a bug as correct behaviour.
    assert_ok("fn main() {\n    x := int(3.0)\n    fmt.println(\"{x}\")\n}\n");
    assert_ok("fn main() {\n    x := u8(200)\n    fmt.println(\"{x}\")\n}\n");
}

#[test]
fn builtin_conversions_stay_accepted() {
    assert_ok("fn main() {\n    x := int(3.0)\n    fmt.println(\"{x}\")\n}\n");
    assert_ok("fn main() {\n    x := u8(200)\n    fmt.println(\"{x}\")\n}\n");
    assert_ok("fn main() {\n    pi := 3.7\n    x := int(pi)\n    fmt.println(\"{x}\")\n}\n");
}

#[test]
fn new_and_make_take_a_type_as_their_first_argument() {
    // This is the case that broke corpus `n5-zero-map-first-write.hy`:
    // `new(bag)` is a type position, and `make`'s first argument likewise.
    // Each binding is READ so the unused-binding rule (HYB-E1002) does not
    // fire and mask the result.
    assert_ok(&format!(
        "{HOLDER}fn main() {{\n    p := new(holder)\n    fmt.println(\"{{p.n}}\")\n}}\n"
    ));
    assert_ok(
        "fn main() {\n    c := make(chan int, 1)\n    c <- 1\n    fmt.println(\"{<-c}\")\n}\n",
    );
    assert_ok("fn main() {\n    s := make([]int, 4)\n    fmt.println(\"{len(s)}\")\n}\n");
    assert_ok("fn main() {\n    s := make([]int, 0, 8)\n    fmt.println(\"{cap(s)}\")\n}\n");
}

#[test]
fn struct_literal_stays_accepted() {
    assert_ok(&format!(
        "{HOLDER}fn main() {{\n    h := holder{{n: 1}}\n    fmt.println(\"{{h.n}}\")\n}}\n"
    ));
}

#[test]
fn a_shadowing_local_is_a_value_not_a_type() {
    // `holder := 5` binds a real variable; the name is a value from then on.
    assert_ok(&format!(
        "{HOLDER}fn main() {{\n    holder := 5\n    x := holder\n    fmt.println(\"{{x}}\")\n}}\n"
    ));
}

#[test]
fn a_field_named_like_a_type_is_fine() {
    assert_ok(&format!(
        "{HOLDER}fn main() {{\n    h := holder{{n: 1}}\n    x := h.n\n    fmt.println(\"{{x}}\")\n}}\n"
    ));
}
