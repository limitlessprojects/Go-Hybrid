//! #1650 — foreign / prelude package call arity.
//!
//! A call into a curated Go package must pass exactly as many arguments as the
//! Go function takes, exactly like a call to a builtin, a conversion, a user
//! `fn`, or a method (all four were already checked). Before this, the foreign
//! path fell through: `hyb check` accepted the program and `go build` refused
//! the emitted call with `not enough arguments in call to time.Since`, a Row 13
//! BUILDABLE violation.
//!
//! Rejects are `HYB-E1002`. The accept cases are the point: a check that closes
//! the bypass by rejecting buildable programs would be a regression, so the
//! variadic targets (`fmt.println`, `filepath.join`, `errors.join`) and the
//! undescribed ones must stay silent.

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

fn assert_rejects_arity(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {cs:?} for:\n{src}"
    );
}

fn assert_accepts(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn too_few_arguments_to_a_foreign_call_is_e1002() {
    // one shape per package.
    assert_rejects_arity("fn main() {\n    d := time.since()\n    fmt.println(\"{d}\")\n}");
    assert_rejects_arity("fn main() {\n    sort.ints()\n}");
    assert_rejects_arity("fn main() {\n    xs := strings.split(\"a\")\n    fmt.println(len(xs))\n}");
    assert_rejects_arity("fn main() {\n    e := errors.new()\n    fmt.println(\"{e}\")\n}");
    assert_rejects_arity("fn main() {\n    n := atomic.load_int64()\n    fmt.println(\"{n}\")\n}");
}

#[test]
fn too_many_arguments_to_a_foreign_call_is_e1002() {
    assert_rejects_arity("fn main() {\n    time.sleep(1, 2)\n}");
    assert_rejects_arity("fn main() {\n    t := time.now(1)\n    fmt.println(\"{t}\")\n}");
    assert_rejects_arity("fn main() {\n    b := strings.contains(\"a\", \"b\", \"c\")\n    fmt.println(b)\n}");
}

#[test]
fn the_diagnostic_names_the_emitted_go_failure() {
    let ast = parse_file(
        "test.hy",
        "fn main() {\n    d := time.since()\n    fmt.println(\"{d}\")\n}",
    )
    .expect("parses");
    let res = check_file(&ast);
    let msg = res
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        msg.contains("time.since") && msg.contains("not enough arguments"),
        "message should name the call and the Go failure, got: {msg}"
    );
}

#[test]
fn correct_foreign_call_arities_accept() {
    assert_accepts("fn main() {\n    d := time.since(time.now())\n    fmt.println(\"{d}\")\n}");
    assert_accepts("fn main() {\n    time.sleep(time.milliseconds(1))\n    fmt.println(\"ok\")\n}");
    assert_accepts("fn main() {\n    t := time.now()\n    fmt.println(\"{t}\")\n}");
    assert_accepts("fn main() {\n    xs := strings.split(\"a,b\", \",\")\n    fmt.println(len(xs))\n}");
    assert_accepts("fn main() {\n    e := errors.new(\"x\")\n    fmt.println(\"{e}\")\n}");
    assert_accepts("fn main() {\n    xs := []int{2, 1}\n    sort.ints(xs)\n    fmt.println(\"{xs[0]}\")\n}");
}

#[test]
fn variadic_targets_stay_unchecked() {
    // `fmt.println` takes any count (including zero) and joins are variadic.
    assert_accepts("fn main() {\n    fmt.println()\n    fmt.println(1, 2, 3)\n}");
    assert_accepts(
        "fn main() {\n    p := filepath.join(\"a\", \"b\", \"c\", \"d\")\n    fmt.println(p)\n}",
    );
    assert_accepts(
        "fn main() {\n    a := errors.new(\"a\")\n    b := errors.new(\"b\")\n    e := errors.join(a, b)\n    fmt.println(\"{e}\")\n}",
    );
}

#[test]
fn undescribed_members_and_type_conversions_stay_silent() {
    // A conversion through a package-qualified type is not a package function
    // and must not be measured against the table.
    assert_accepts("fn main() {\n    d := time.duration(5)\n    fmt.println(\"{d}\")\n}");
}

#[test]
fn a_lone_multi_value_argument_expands_to_its_result_count() {
    // `f(g())` with `g` returning two values is two effective arguments, so it
    // must be compared as two, not as one: written `args.len()` is 1 here and
    // measuring that against `strings.join`'s 2 would wrongly reject.
    assert_accepts(
        "fn pair() (string, int) { return \"a\", 0 }\nfn main() {\n    s := strings.join(pair())\n    fmt.println(s)\n}",
    );
}
