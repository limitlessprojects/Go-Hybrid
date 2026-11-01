//! tuple_forwarded_and_const_arity.rs — regression tests for T23 (HYB-E2002)
//! multi-value arity checks across:
//!   1. inner subexpressions of legally forwarded multi-value call/task arguments,
//!   2. file-level `const` initializers (including nested closure bodies),
//!   3. `move`-wrapped, foreign-package, indirect, and aliased multi-value returns
//!      and forwarded call arguments.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn assert_e2002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(
        cs.iter().any(|c| c == "HYB-E2002"),
        "expected HYB-E2002 in {cs:?} for:\n{src}"
    );
}

fn assert_only_e2002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert_eq!(
        cs,
        vec!["HYB-E2002".to_string()],
        "expected only [HYB-E2002], got {cs:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected 0 errors, got {:?} for:\n{src}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn inner_subexprs_of_legally_forwarded_call_and_task_args_are_checked() {
    // Call forwarding: outer `cb` takes 2 args and `pair` returns 2 values,
    // so `pair(...)` forwards legally, but `pair`'s first argument is a
    // single-value position and must reject a 2-value call.
    assert_e2002(
        "import \"strconv\"\n\
         fn pair(a i64, b int) (i64, int) { return a, b }\n\
         fn main() {\n\
             cb := fn(x i64, y int) { _ = x\n _ = y }\n\
             cb(pair(strconv.parse_int(\"1\", 10, 64), 2))\n\
         }\n",
    );

    // Task spawn forwarding: outer `cb` takes 2 args and `pair` returns 2 values,
    // while `pair`'s inner argument is a 2-value foreign call.
    assert_e2002(
        "import \"strconv\"\n\
         fn pair(a i64, b int) (i64, int) { return a, b }\n\
         fn main() {\n\
             cb := fn(x i64, y int) { _ = x\n _ = y }\n\
             task cb(pair(strconv.parse_int(\"1\", 10, 64), 2))\n\
         }\n",
    );

    // Inline closure forwarding with user 2-value call inside inner argument.
    assert_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn pair(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
             (fn(x int, y int) { _ = x\n _ = y })(pair(minmax(1, 2), 3))\n\
         }\n",
    );
}

#[test]
fn file_level_const_closures_and_composites_reject_multi_value_binds() {
    // Untyped slice literal in file-level `const` holding a closure with a 1-target bind of a 2-value call.
    assert_only_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         const fns = [fn() {\n\
             p := minmax(3, 9)\n\
             _ = p\n\
         }]\n\
         fn main() {\n\
             _ = fns\n\
         }\n",
    );

    // Typed slice literal in file-level `const` holding a closure with a foreign 2-value call.
    assert_only_e2002(
        "import \"strconv\"\n\
         const fns = []fn(){fn() {\n\
             p := strconv.parse_int(\"1\", 10, 64)\n\
             _ = p\n\
         }}\n\
         fn main() {\n\
             _ = fns\n\
         }\n",
    );

    // Struct literal in file-level `const` with alias chain inside closure body.
    assert_only_e2002(
        "struct holder { cb fn() }\n\
         fn minmax(a int, b int) (int, int) { return a, b }\n\
         const h = holder{cb: fn() {\n\
             mm := minmax\n\
             m2 := mm\n\
             p := m2(3, 9)\n\
             _ = p\n\
         }}\n\
         fn main() {\n\
             _ = h\n\
         }\n",
    );

    // Direct multi-value call inside a composite `const` element.
    assert_e2002(
        "import \"strconv\"\n\
         const xs = []i64{strconv.parse_int(\"1\", 10, 64)}\n\
         fn main() {\n\
             _ = xs\n\
         }\n",
    );
}

#[test]
fn single_value_return_and_forwarded_calls_reject_move_foreign_indirect_and_aliased_callees() {
    // 1. `return move minmax(3, 9)` in a 1-result function.
    assert_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn bad() int { return move minmax(3, 9) }\n\
         fn main() { _ = bad() }\n",
    );

    // 2. `return strconv.parse_int(...)` in a 1-result function.
    assert_only_e2002(
        "import \"strconv\"\n\
         fn bad() i64 { return strconv.parse_int(\"42\", 10, 64) }\n\
         fn main() { _ = bad() }\n",
    );

    // 3. `return fns[0](3, 9)` in a 1-result function (typed slice local and untyped slice const).
    assert_only_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn bad() int {\n\
             fns := []fn(a int, b int) int int{minmax}\n\
             return fns[0](3, 9)\n\
         }\n\
         fn main() { _ = bad() }\n",
    );
    assert_only_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         const fns = [minmax]\n\
         fn bad() int {\n\
             return fns[0](3, 9)\n\
         }\n\
         fn main() { _ = bad() }\n",
    );

    // 4. `sink(move minmax(3, 9))` and `sink(strconv.parse_int("42", 10, 64))` on a 1-param top-level fn.
    assert_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn sink(x int) { _ = x }\n\
         fn main() { sink(move minmax(3, 9)) }\n",
    );
    assert_only_e2002(
        "import \"strconv\"\n\
         fn sink(x i64) { _ = x }\n\
         fn main() { sink(strconv.parse_int(\"42\", 10, 64)) }\n",
    );

    // 5. Aliased closure callee `cb2 := cb1; cb2(minmax(3, 9))` and user method `b.take_one(minmax(3, 9))`.
    assert_e2002(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
             cb1 := fn(x int) { _ = x }\n\
             var cb2 any = 1\n\
             cb2 = cb1\n\
             cb2(minmax(3, 9))\n\
         }\n",
    );
    assert_e2002(
        "struct box { v int }\n\
         fn (b box) take_one(x int) { _ = b.v\n _ = x }\n\
         fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
             b := box{v: 1}\n\
             b.take_one(minmax(3, 9))\n\
         }\n",
    );
}

#[test]
fn valid_forwarding_and_const_destructuring_stay_accepted() {
    assert_accept(
        "import \"strconv\"\n\
         fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn w(x int, y int) { _ = x\n _ = y }\n\
         fn fwd() (i64, error) { return strconv.parse_int(\"42\", 10, 64) }\n\
         const fns = [fn() {\n\
             a, b := minmax(3, 9)\n\
             _ = a\n\
             _ = b\n\
         }]\n\
         fn main() {\n\
             (fn(x int, y int) { _ = x\n _ = y })(minmax(1, 2))\n\
             w(minmax(1, 2))\n\
             task w(minmax(3, 4))\n\
             v, err := fwd()\n\
             _ = v\n\
             _ = err\n\
             _ = fns\n\
         }\n",
    );
}
