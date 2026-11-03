//! issue_3412_composite_literal_inner_types.rs — issue #3412.
//!
//! GUARANTEES.md Row 13 (BUILDABLE) requires `hyb check` to reject a program
//! that fails `go build`. `check_slice_lit_items`, `check_array_lit_items`
//! and `check_map_lit_entries` compared coarse `OpKind`s only, and `OpKind`
//! collapses every integer width into `Int`, both float widths into `Float`,
//! and — crucially — the *inner* type of a container into the same node
//! (`OpKind::Chan(Int)` for both `chan int32` and `chan int64`). A composite
//! literal whose element/key/value is a container of the wrong inner type
//! therefore produced 0 diagnostics while `go build` refused it:
//!
//!     cannot use bad_chan (variable of type chan int64)
//!         as chan int32 value in array or slice literal
//!
//! The checks now also compare the *declared spellings* of both sides when
//! both are provably typed, which is the only evidence the kind model drops.
//!
//! Every reject case below was reproduced against `go build` on the emitted
//! Go before the fix (see the issue); every accept case is a program Go
//! builds, and is here as a negative control so the comparison cannot turn
//! into a blanket spelling equality.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {codes:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "{what}");
}

fn assert_ok(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean accept for {what}, got {:?}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Slice literals — `[]chan int32{bad_chan}` with `bad_chan chan int64`
// ---------------------------------------------------------------------------
#[test]
fn slice_literal_chan_inner_width_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad chan int64\n\
           let s = []chan int32{ bad }\n\
           fmt.println(\"{s}\")\n\
         }",
        "[]chan int32{ bad } where bad is chan int64",
    );
}

#[test]
fn slice_literal_array_inner_len_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad [4]int\n\
           let s = [][3]int{ bad }\n\
           fmt.println(\"{s}\")\n\
         }",
        "[][3]int{ bad } where bad is [4]int",
    );
}

#[test]
fn slice_literal_map_inner_value_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad map[string]int64\n\
           let s = []map[string]int32{ bad }\n\
           fmt.println(\"{s}\")\n\
         }",
        "[]map[string]int32{ bad } where bad is map[string]int64",
    );
}

#[test]
fn slice_literal_slice_inner_width_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad []int64\n\
           let s = [][]int32{ bad }\n\
           fmt.println(\"{s}\")\n\
         }",
        "[][]int32{ bad } where bad is []int64",
    );
}

// ---------------------------------------------------------------------------
// Array literals — `[1]chan int32{bad_chan}`
// ---------------------------------------------------------------------------
#[test]
fn array_literal_chan_inner_width_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad chan int64\n\
           let a = [1]chan int32{ bad }\n\
           fmt.println(\"{a}\")\n\
         }",
        "[1]chan int32{ bad } where bad is chan int64",
    );
}

#[test]
fn array_literal_array_inner_len_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad [4]int\n\
           let a = [1][3]int{ bad }\n\
           fmt.println(\"{a}\")\n\
         }",
        "[1][3]int{ bad } where bad is [4]int",
    );
}

// ---------------------------------------------------------------------------
// Map literals — key and value positions
// ---------------------------------------------------------------------------
#[test]
fn map_literal_value_chan_inner_width_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad chan int64\n\
           let m = map[string]chan int32{ \"k\": bad }\n\
           fmt.println(\"{m}\")\n\
         }",
        "map[string]chan int32{\"k\": bad} where bad is chan int64",
    );
}

#[test]
fn map_literal_key_chan_inner_width_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad chan int64\n\
           let m = map[chan int32]string{ bad: \"v\" }\n\
           fmt.println(\"{m}\")\n\
         }",
        "map[chan int32]string{bad: \"v\"} where bad is chan int64",
    );
}

#[test]
fn map_literal_value_array_inner_len_rejected() {
    assert_rejects_e1002(
        "fn main() {\n\
           var bad [4]int\n\
           let m = map[string][3]int{ \"k\": bad }\n\
           fmt.println(\"{m}\")\n\
         }",
        "map[string][3]int{\"k\": bad} where bad is [4]int",
    );
}

// ---------------------------------------------------------------------------
// Negative controls — programs Go BUILDS must stay accepted.
// ---------------------------------------------------------------------------
#[test]
fn accept_matching_inner_types() {
    assert_ok(
        "fn main() {\n\
           var c chan int32\n\
           let s = []chan int32{ c }\n\
           var a [3]int\n\
           let b = [1][3]int{ a }\n\
           let m = map[string]chan int32{ \"k\": c }\n\
           fmt.println(\"{s}{b}{m}\")\n\
         }",
        "every inner type matches its literal",
    );
}

#[test]
fn accept_numeric_alias_pairs() {
    // `byte`/`uint8` and `rune`/`int32` are the same Go type.
    assert_ok(
        "fn main() {\n\
           var b byte\n\
           let s = []uint8{ b }\n\
           var r rune\n\
           let t = []int32{ r }\n\
           fmt.println(\"{s}{t}\")\n\
         }",
        "byte/uint8 and rune/int32 aliases",
    );
}

#[test]
fn accept_any_element_type() {
    // An `any` element type accepts any value.
    assert_ok(
        "fn main() {\n\
           var bad chan int64\n\
           let s = []any{ bad }\n\
           fmt.println(\"{s}\")\n\
         }",
        "[]any{...} accepts a container value",
    );
}

#[test]
fn accept_untyped_int_into_float_container() {
    // An untyped int constant is assignable to a float element type; the
    // kind model cannot tell it from a typed `int`, so this direction stays
    // allowed.
    assert_ok(
        "fn main() {\n\
           let s = []float64{ 1 }\n\
           fmt.println(\"{s}\")\n\
         }",
        "untyped int constant into []float64",
    );
}

#[test]
fn accept_named_struct_elems() {
    // A struct element type is not a spelling comparison candidate.
    assert_ok(
        "struct p { n int }\n\
         fn main() {\n\
           let s = []p{ p{n: 1} }\n\
           fmt.println(\"{s}\")\n\
         }",
        "[]p{ p{n: 1} }",
    );
}
