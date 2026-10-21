//! array_len_const_1133.rs — regressions for issue #1133: `len`/`cap` calls
//! over a non-constant operand in an array-length position passed `hyb check`
//! with 0 diagnostics, then failed `go build` with `invalid array length`.
//!
//! Go folds `len(s)` / `cap(s)` to a constant only when `s` is an array or a
//! pointer to an array (`len(s)` additionally folds for a string *constant*).
//! A channel, slice, map, struct, or a string/primitive *variable* operand can
//! therefore never be a constant array length, so `check_array_len` must fail
//! closed on those instead of taking the section-4 fail-open.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts_without(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == code),
        "{} must not fire, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept, "src:\n{}", src);
}

// ---------------------------------------------------------------------------
// Non-constant operands — must reject HYB-E6004
// ---------------------------------------------------------------------------

const LEN_CHAN: &str = "\
fn main() {
    ch := make(chan int, 4)
    var xs [len(ch)]int
    _ = xs
}
";

const CAP_CHAN: &str = "\
fn main() {
    ch := make(chan int, 4)
    var xs [cap(ch)]int
    _ = xs
}
";

const LEN_SLICE: &str = "\
fn main() {
    ys := []int{1, 2}
    var xs [len(ys)]int
    _ = xs
}
";

const LEN_STRING_VAR: &str = "\
fn main() {
    s := \"hello\"
    var xs [len(s)]int
    _ = xs
}
";

const CAP_CHAN_ARITH: &str = "\
fn main() {
    ch := make(chan int, 2)
    var xs [cap(ch)-1]int
    _ = xs
}
";

const LEN_MAP: &str = "\
fn main() {
    m := map[string]int{\"a\": 1}
    var xs [len(m)]int
    _ = xs
}
";

#[test]
fn len_over_channel_is_rejected() {
    rejects_with(LEN_CHAN, "HYB-E6004");
}

#[test]
fn cap_over_channel_is_rejected() {
    rejects_with(CAP_CHAN, "HYB-E6004");
}

#[test]
fn len_over_slice_is_rejected() {
    rejects_with(LEN_SLICE, "HYB-E6004");
}

#[test]
fn len_over_string_variable_is_rejected() {
    rejects_with(LEN_STRING_VAR, "HYB-E6004");
}

#[test]
fn arithmetic_over_cap_call_is_rejected() {
    rejects_with(CAP_CHAN_ARITH, "HYB-E6004");
}

#[test]
fn len_over_map_is_rejected() {
    rejects_with(LEN_MAP, "HYB-E6004");
}

// ---------------------------------------------------------------------------
// Constant operands — must stay accepted (the fail-open that remains)
// ---------------------------------------------------------------------------

const LITERAL_LEN_OK: &str = "\
fn main() {
    var xs [3]int
    _ = xs
}
";

const FOLDED_ARITH_LEN_OK: &str = "\
fn main() {
    var xs [1+2]int
    _ = xs
}
";

const LEN_STRING_LITERAL_OK: &str = "\
fn main() {
    var xs [len(\"hello\")]int
    _ = xs
}
";

const LEN_CONCAT_LITERAL_OK: &str = "\
fn main() {
    var xs [len(\"a\"+\"b\")]int
    _ = xs
}
";

const LEN_FILE_CONST_STRING_OK: &str = "\
const s = \"hello\"
fn main() {
    var xs [len(s)]int
    _ = xs
}
";

const LEN_ARRAY_LOCAL_OK: &str = "\
fn main() {
    var a [3]int
    var b [len(a)]int
    _ = a
    _ = b
}
";

const CAP_ARRAY_LOCAL_OK: &str = "\
fn main() {
    var a [3]int
    var b [cap(a)]int
    _ = a
    _ = b
}
";

const LEN_POINTER_TO_ARRAY_OK: &str = "\
fn main() {
    var a [3]int
    p := &a
    var b [len(p)]int
    _ = b
}
";

const LEN_LITERAL_ARRAY_INIT_OK: &str = "\
fn main() {
    xs := [len(\"hello\")]int{}
    _ = xs
}
";

#[test]
fn literal_array_length_stays_accepted() {
    accepts_without(LITERAL_LEN_OK, "HYB-E6004");
}

#[test]
fn folded_constant_arithmetic_length_stays_accepted() {
    accepts_without(FOLDED_ARITH_LEN_OK, "HYB-E6004");
}

#[test]
fn len_of_string_literal_stays_accepted() {
    accepts_without(LEN_STRING_LITERAL_OK, "HYB-E6004");
}

#[test]
fn len_of_folded_string_concat_stays_accepted() {
    accepts_without(LEN_CONCAT_LITERAL_OK, "HYB-E6004");
}

#[test]
fn len_of_file_const_string_stays_accepted() {
    accepts_without(LEN_FILE_CONST_STRING_OK, "HYB-E6004");
}

#[test]
fn len_of_array_local_stays_accepted() {
    accepts_without(LEN_ARRAY_LOCAL_OK, "HYB-E6004");
}

#[test]
fn cap_of_array_local_stays_accepted() {
    accepts_without(CAP_ARRAY_LOCAL_OK, "HYB-E6004");
}

#[test]
fn len_of_pointer_to_array_stays_accepted() {
    accepts_without(LEN_POINTER_TO_ARRAY_OK, "HYB-E6004");
}

#[test]
fn len_of_string_literal_in_array_literal_stays_accepted() {
    accepts_without(LEN_LITERAL_ARRAY_INIT_OK, "HYB-E6004");
}
