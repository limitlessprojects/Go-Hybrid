//! option_set_and_method_scalar_widths.rs — regression tests for numeric
//! scalar width checks on `set[T]` methods (`insert`, `contains`, `remove`),
//! `option[T]` methods and pattern bindings (`or`, `unwrap`, `match some(v)`),
//! `slice.pop()`, and struct method call results (`GUARANTEES.md` Row 13 `BUILDABLE`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("test program must parse");
    check_file(&ast)
}

fn assert_rejects_single_e1002(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject, got {:?}: {:?}",
        r.verdict,
        r.diagnostics
    );
    let errs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(
        errs.len(),
        1,
        "expected exactly 1 error diagnostic, got {:?}",
        errs
    );
    assert_eq!(errs[0].code, "HYB-E1002", "expected HYB-E1002, got {:?}", errs[0]);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?}: {:?}",
        r.verdict,
        r.diagnostics
    );
}

#[test]
fn set_insert_contains_remove_mismatched_numeric_width_rejects() {
    assert_rejects_single_e1002(
        r#"
fn main() {
    s := set[int64]{}
    var x int32 = 1
    s.insert(x)
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
fn main() {
    var s set[uint32] = set[uint32]{}
    var x int32 = 1
    _ = s.contains(x)
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
struct holder {
    ids set[int64]
}

fn main() {
    h := holder{ids: set[int64]{}}
    var x int = 7
    h.ids.remove(x)
}
"#,
    );
}

#[test]
fn option_or_mismatched_numeric_width_rejects() {
    assert_rejects_single_e1002(
        r#"
fn main() {
    var opt option[int64] = some(int64(10))
    var d int32 = 5
    _ = opt.or(d)
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
fn main() {
    opt := some(int64(10))
    var d int = 5
    _ = opt.or(d)
}
"#,
    );
}

#[test]
fn option_unwrap_and_or_result_scalar_width_rejects_mismatch() {
    assert_rejects_single_e1002(
        r#"
fn main() {
    var opt option[int64] = some(int64(10))
    var x int32 = opt.unwrap()
    _ = x
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
fn main() {
    opt := some(int64(10))
    v := opt.or(0)
    var x int32 = v
    _ = x
}
"#,
    );
}

#[test]
fn match_option_some_binding_propagates_scalar_width() {
    assert_rejects_single_e1002(
        r#"
fn main() {
    opt := some(int64(42))
    match opt {
    some(v) => {
        var x int32 = v
        _ = x
    }
    none => {}
    }
}
"#,
    );
}

#[test]
fn struct_method_call_and_slice_pop_result_scalars_reject_mismatch() {
    assert_rejects_single_e1002(
        r#"
struct counter {
    n int64
}

fn (c counter) get() int64 {
    return c.n
}

fn main() {
    c := counter{n: 1}
    var x int32 = c.get()
    _ = x
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
struct counter {
    n int64
}

fn (c counter) pair() (int64, bool) {
    return c.n, true
}

fn main() {
    c := counter{n: 1}
    v, ok := c.pair()
    var x int32 = v
    _ = x
    _ = ok
}
"#,
    );

    assert_rejects_single_e1002(
        r#"
fn main() {
    s := []int64{1, 2}
    var x int32 = s.pop()
    _ = x
}
"#,
    );
}

#[test]
fn matching_widths_untyped_constants_and_shadowing_accept() {
    assert_accepts(
        r#"
struct counter {
    n int64
}

fn (c counter) get() int64 {
    return c.n
}

fn main() {
    s := set[int64]{}
    var x int64 = 10
    s.insert(x)
    s.insert(42)
    _ = s.contains(x)
    s.remove(x)

    opt := some(int64(10))
    var u int64 = opt.unwrap()
    var o int64 = opt.or(0)
    var o2 int64 = opt.or(x)
    _ = u
    _ = o
    _ = o2

    match opt {
    some(v) => {
        var mv int64 = v
        _ = mv
    }
    none => {}
    }

    c := counter{n: 5}
    var cv int64 = c.get()
    _ = cv
}
"#,
    );

    // Inner-scope shadowing of `opt` and `s` with a different element width must not leak outer width
    assert_accepts(
        r#"
fn main() {
    var opt option[int64] = some(int64(1))
    var s set[int64] = set[int64]{}
    if true {
        opt := some(int32(2))
        s := set[int32]{}
        var x int32 = 3
        s.insert(x)
        var y int32 = opt.or(x)
        _ = y
    }
    var ox int64 = 9
    s.insert(ox)
    _ = opt.or(ox)
}
"#,
    );
}
