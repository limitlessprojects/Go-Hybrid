//! Issue #3314 (GUARANTEES Row 9, A5/C9): HYB-E3013 fired only when the
//! closure was handed to a FOREIGN callee (`time.after_func(0, fn(){...})`).
//! One user function in between laundered the write and the checker accepted
//! a program with a real data race, confirmed under `go run -race`:
//!
//! ```text
//! WARNING: DATA RACE
//!   Read at 0x... by goroutine 8:  runtime.mapaccess1_faststr()
//!   Previous write by main goroutine:  main.bump()
//! ```
//!
//! The fix makes the reachability structural: a user function whose parameter
//! (or receiver) reaches foreign Go code, or is written through, is an escape
//! site, and the capture walk treats a call into such a function as a write.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e3013(src: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
    assert!(
        cs.iter().any(|c| c == "HYB-E3013"),
        "expected HYB-E3013 for:\n{src}\ngot {cs:?}"
    );
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.is_empty(),
        "must accept:\n{src}\ngot {:?}",
        r.diagnostics
    );
}

#[test]
fn a_closure_laundered_through_a_user_function_is_an_escape_site() {
    // The issue's shape: `later` forwards its `fn()` parameter into
    // `time.after_func`.
    assert_e3013(
        "import \"time\"\n\
         fn later(f fn()) {\n\
             _ = time.after_func(0, f)\n\
         }\n\
         fn main() {\n\
             m := map[string]int{}\n\
             later(fn() { m[\"timer\"] = 1 })\n\
             m[\"main\"] = 2\n\
         }\n",
    );

    // Wrapper of a wrapper.
    assert_e3013(
        "import \"time\"\n\
         fn later(f fn()) { _ = time.after_func(0, f) }\n\
         fn later2(f fn()) { later(f) }\n\
         fn main() {\n\
             m := map[string]int{}\n\
             later2(fn() { m[\"t\"] = 1 })\n\
         }\n",
    );

    // A local alias inside the wrapper: `g := f` then forward `g`.
    assert_e3013(
        "import \"time\"\n\
         fn later(f fn()) {\n\
             g := f\n\
             _ = time.after_func(0, g)\n\
         }\n\
         fn main() {\n\
             m := map[string]int{}\n\
             later(fn() { m[\"t\"] = 1 })\n\
         }\n",
    );

    // A pointer-receiver METHOD forwarding its parameter.
    assert_e3013(
        "import \"time\"\n\
         struct sched { n int }\n\
         fn (s *sched) defer_run(f fn()) {\n\
             _ = time.after_func(0, f)\n\
             s.n = s.n + 1\n\
         }\n\
         fn main() {\n\
             var s sched\n\
             m := map[string]int{}\n\
             s.defer_run(fn() { m[\"t\"] = 1 })\n\
         }\n",
    );
}

#[test]
fn a_user_function_that_writes_through_its_parameter_mutates_the_capture() {
    // The issue's second half: the closure itself calls `bump(m, k)`, and
    // `bump` writes through `m`. The capture walk used to see no write.
    assert_e3013(
        "import \"time\"\n\
         fn bump(m map[string]int, k string) { m[k] = m[k] + 1 }\n\
         fn main() {\n\
             m := map[string]int{}\n\
             _ = time.after_func(0, fn() { bump(m, \"timer\") })\n\
             bump(m, \"main\")\n\
         }\n",
    );

    // Same, reached through a wrapper on both ends.
    assert_e3013(
        "import \"time\"\n\
         fn bump(m map[string]int, k string) { m[k] = m[k] + 1 }\n\
         fn later(f fn()) { _ = time.after_func(0, f) }\n\
         fn main() {\n\
             m := map[string]int{}\n\
             later(fn() { bump(m, \"timer\") })\n\
         }\n",
    );

    // A pointer-receiver method that writes a field of the captured struct.
    assert_e3013(
        "import \"time\"\n\
         struct stats { n int }\n\
         fn (s *stats) incr() { s.n = s.n + 1 }\n\
         fn main() {\n\
             s := stats{n: 0}\n\
             _ = time.after_func(0, fn() { s.incr() })\n\
             s.incr()\n\
         }\n",
    );

    // A pointer taken at the call site.
    assert_e3013(
        "import \"time\"\n\
         struct stats { n int }\n\
         fn bump(s *stats) { s.n = s.n + 1 }\n\
         fn later(f fn()) { _ = time.after_func(0, f) }\n\
         fn main() {\n\
             s := stats{n: 0}\n\
             later(fn() { bump(&s) })\n\
         }\n",
    );
}

#[test]
fn the_direct_site_and_the_managed_hatch_are_unchanged() {
    // The direct foreign call still fires (this was already true).
    assert_e3013(
        "import \"time\"\n\
         fn main() {\n\
             m := map[string]int{}\n\
             _ = time.after_func(0, fn() { m[\"t\"] = 1 })\n\
         }\n",
    );

    // Negative controls: a user function that does NOT reach foreign code must
    // not become an escape site just because it takes a `fn()`.
    assert_accept(
        "fn later(f fn()) { f() }\n\
         fn main() {\n\
             m := map[string]int{}\n\
             later(fn() { _ = m[\"t\"] })\n\
         }\n",
    );
    assert_accept(
        "fn keep(f fn()) { _ = f }\n\
         fn main() {\n\
             m := map[string]int{}\n\
             keep(fn() { m[\"t\"] = 1 })\n\
         }\n",
    );
    // A user function that mutates its own LOCAL, not the parameter.
    assert_accept(
        "import \"time\"\n\
         fn later(f fn()) {\n\
             var local int = 0\n\
             local = local + 1\n\
             _ = local\n\
             _ = time.after_func(0, f)\n\
         }\n\
         fn main() {\n\
             x := 1\n\
             later(fn() { _ = x })\n\
         }\n",
    );
}
