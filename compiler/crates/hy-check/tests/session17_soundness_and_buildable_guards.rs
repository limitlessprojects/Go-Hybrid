//! session17_soundness_and_buildable_guards.rs — regression tests for
//! soundness and buildable-Go guard bypasses and false positives across:
//!   * HYB-E3104 (T24: map-slot loan address-of on parenthesized slot loans,
//!     field projections `&n.val`, and closure bodies)
//!   * HYB-E6003 (issue #205: shadowed file `const`, composite `const`
//!     references, self-referential/cyclic file `const`, forward-referenced
//!     file `const`, conversion arity/shadowing, `len("...")`, closure `const`)
//!   * HYB-E2008 / HYB-E2009 / HYB-E3007 (nested assignment-place receives,
//!     closure `if`-init receives, channel direction on send/recv/range,
//!     two-variable channel range `for i, v in ch`)
//!   * HYB-E2002 (T23: closure-body multi-value calls and multi-value calls
//!     inside multi-expression RHS lists)
//!   * HYB-E2006 / HYB-E6004 (closure-scoped equality type environment,
//!     shadowed/non-int/local `const` array lengths, composite literal types)
//!   * HYB-E1001 (struct type parameter scope isolation and bare undefined
//!     `MatchPattern::Path` arm identifiers)

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("snippet must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects(src: &str, expected: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject ({expected}) for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    let matching: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == expected)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly 1 {expected} diagnostic (zero duplicates) for:\n{src}\ngot codes {codes:?}: {:?}",
        r.diagnostics
    );
    let span = matching[0].span;
    assert!(
        span.start.line >= 1
            && span.start.col >= 1
            && (span.end.line, span.end.col) >= (span.start.line, span.start.col),
        "expected valid ordered source span on {expected}, got {:?}",
        span
    );
}

fn assert_rejects_at_line(src: &str, expected: &str, expected_line: u32) {
    assert_rejects(src, expected);
    let r = check(src);
    let diag = r
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error && d.code == expected)
        .expect("verified by assert_rejects");
    assert_eq!(
        diag.span.start.line, expected_line,
        "expected {expected} at line {expected_line}, got span {:?}",
        diag.span
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).is_empty(),
        "unexpected errors for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 1. HYB-E3104 (T24: no address-of on a map-slot loan)
// ---------------------------------------------------------------------------

#[test]
fn t24_parenthesized_slot_loan_local_and_assign_reject_e3104() {
    assert_rejects(
        "fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           n1 := &(m[\"k\"])\n\
           p1 := &n1\n\
           fmt.println(p1)\n\
         }\n",
        "HYB-E3104",
    );
    assert_rejects(
        "fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           n2 := (&m[\"k\"])\n\
           p2 := &n2\n\
           fmt.println(p2)\n\
         }\n",
        "HYB-E3104",
    );
    assert_rejects(
        "fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           n := 0\n\
           n = &(m[\"k\"])\n\
           p := &n\n\
           fmt.println(p)\n\
         }\n",
        "HYB-E3104",
    );
}

#[test]
fn t24_field_projection_and_closure_addr_of_loan_reject_e3104() {
    // Field projection `&n.val` on a map-slot loan `n := &m["k"]`.
    assert_rejects(
        "struct node { val int }\n\
         fn main() {\n\
           m := map[string]node{\"k\": node{val: 1}}\n\
           n := &m[\"k\"]\n\
           p := &n.val\n\
           fmt.println(p)\n\
         }\n",
        "HYB-E3104",
    );

    // Closure body taking `&n` on a captured or closure-local map-slot loan.
    assert_rejects(
        "fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           n := &m[\"k\"]\n\
           f := fn() {\n\
             p := &n\n\
             fmt.println(p)\n\
           }\n\
           f()\n\
         }\n",
        "HYB-E3104",
    );
    assert_rejects(
        "fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           f := fn() {\n\
             n := &m[\"k\"]\n\
             p := &n\n\
             fmt.println(p)\n\
           }\n\
           f()\n\
         }\n",
        "HYB-E3104",
    );

    // Closure `if`-init defining a slot loan `if n := &m["k"]; check_ptr(&n)`:
    // the condition runs AFTER `init` and must reject `&n` with HYB-E3104.
    assert_rejects(
        "fn check_ptr(p *int) bool { return p != nil }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           f := fn() {\n\
             if n := &m[\"k\"]; check_ptr(&n) {\n\
               fmt.println(\"hit\")\n\
             }\n\
           }\n\
           f()\n\
         }\n",
        "HYB-E3104",
    );

    // Positive control: closure `if`-init shadowing an outer slot loan `n`
    // with a plain value `n := 1` must NOT falsely reject `&n` in `cond`.
    assert_accepts(
        "fn check_ptr(p *int) bool { return p != nil }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           n := &m[\"k\"]\n\
           fmt.println(n)\n\
           f := fn() {\n\
             if n := 1; check_ptr(&n) {\n\
               fmt.println(n)\n\
             }\n\
           }\n\
           f()\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// 2. HYB-E6003 (non-constant `const` initializers & false positives)
// ---------------------------------------------------------------------------

#[test]
fn const_init_shadowed_composite_and_self_ref_reject_e6003() {
    // File-level `const base = 10` shadowed by parameter or mutable local.
    assert_rejects(
        "const base = 10\n\
         fn f(base int) {\n\
           const c = base + 1\n\
           fmt.println(c)\n\
         }\n\
         fn main() { f(3) }\n",
        "HYB-E6003",
    );

    // Referencing a composite `const` (lowered to Go `var`) inside a scalar `const`.
    assert_rejects(
        "struct pair { a int\n b int }\n\
         const p = pair{a: 1, b: 2}\n\
         const c = p\n\
         fn main() { fmt.println(c.a) }\n",
        "HYB-E6003",
    );

    // Self-referential file-level `const c = c + 1`.
    assert_rejects(
        "const c = c + 1\n\
         fn main() { fmt.println(c) }\n",
        "HYB-E6003",
    );

    // Conversion arity and shadowed conversion name.
    assert_rejects(
        "fn main() {\n\
           const a = int8()\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6003",
    );
    assert_rejects(
        "fn main() {\n\
           const a = int8(1, 2)\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6003",
    );
    assert_rejects(
        "fn main() {\n\
           int8 := fn(x int) int { return x }\n\
           const c = int8(1)\n\
           fmt.println(c)\n\
         }\n",
        "HYB-E6003",
    );

    // Closure-body `const` initialized from a runtime call.
    assert_rejects(
        "fn get_val() int { return 7 }\n\
         fn main() {\n\
           f := fn() {\n\
             const c = get_val()\n\
             fmt.println(c)\n\
           }\n\
           f()\n\
         }\n",
        "HYB-E6003",
    );
}

#[test]
fn const_init_forward_ref_and_len_const_str_accept() {
    assert_accepts(
        "const a = b + 1\n\
         const b = 2\n\
         const n = len(\"hybrid\")\n\
         const deep_sum = 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10 + 11 + 12\n\
         fn main() {\n\
           fmt.println(\"{a} {b} {n} {deep_sum}\")\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// 3. HYB-E2008 / HYB-E2009 / HYB-E3007 (channels & range)
// ---------------------------------------------------------------------------

#[test]
fn channel_direction_nested_place_recv_and_two_var_chan_range_reject() {
    // Nested assignment place index receive `arr[<-bad].val = 10`.
    assert_rejects(
        "struct item { val int }\n\
         fn main() {\n\
           arr := []item{item{val: 1}}\n\
           bad := 0\n\
           arr[<-bad].val = 10\n\
           fmt.println(arr[0].val)\n\
         }\n",
        "HYB-E2008",
    );

    // Receive from send-only channel `chan<- int` -> HYB-E3008
    // (chan-direction-mismatch owns directed-channel misuse since #443;
    // the generic E2008 arm yields to it on the same site).
    assert_rejects(
        "fn sink(out chan<- int) {\n\
           x := <-out\n\
           fmt.println(x)\n\
         }\n\
         fn main() {\n\
           ch := make(chan int, 1)\n\
           sink(ch)\n\
         }\n",
        "HYB-E3008",
    );

    // Send to receive-only channel `<-chan int` -> HYB-E3008
    // (chan-direction-mismatch owns directed-channel misuse since #443;
    // the generic E3007 arm yields to it on the same site).
    assert_rejects(
        "fn source(inp <-chan int) {\n\
           inp <- 1\n\
         }\n\
         fn main() {\n\
           ch := make(chan int, 1)\n\
           source(ch)\n\
         }\n",
        "HYB-E3008",
    );

    // Range over send-only channel `chan<- int` -> HYB-E2009.
    assert_rejects(
        "fn sink(out chan<- int) {\n\
           for x in out {\n\
             fmt.println(x)\n\
           }\n\
         }\n\
         fn main() {\n\
           ch := make(chan int, 1)\n\
           sink(ch)\n\
         }\n",
        "HYB-E2009",
    );

    // Two-variable range over channel `for i, v in ch` -> HYB-E2009.
    assert_rejects(
        "fn main() {\n\
           ch := make(chan int, 1)\n\
           close(ch)\n\
           for i, v in ch {\n\
             fmt.println(\"{i} {v}\")\n\
           }\n\
         }\n",
        "HYB-E2009",
    );

    // Positive control: closure `if`-init defining a channel and receiving in `cond`.
    assert_accepts(
        "fn main() {\n\
           f := fn() {\n\
             if ch := make(chan int, 1); (<-ch) >= 0 {\n\
               fmt.println(\"ok\")\n\
             }\n\
           }\n\
           f()\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// 4. HYB-E2002 (T23: multi-value call in closures & multi-expr lists)
// ---------------------------------------------------------------------------

#[test]
fn multi_value_in_closure_and_multi_expr_list_reject_e2002() {
    assert_rejects(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
           f := fn() {\n\
             p := minmax(3, 9)\n\
             fmt.println(p)\n\
           }\n\
           f()\n\
         }\n",
        "HYB-E2002",
    );
    assert_rejects(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
           a, b, c := minmax(1, 2), 3\n\
           fmt.println(\"{a} {b} {c}\")\n\
         }\n",
        "HYB-E2002",
    );
}

// ---------------------------------------------------------------------------
// 5. HYB-E2006 & HYB-E6004 (closure equality scope & array lengths)
// ---------------------------------------------------------------------------

#[test]
fn closure_equality_scope_and_array_len_guards() {
    // Closure parameter/local of incomparable type rejects with HYB-E2006.
    assert_rejects(
        "fn main() {\n\
           f := fn(m map[string]int) {\n\
             if m == m {\n\
               fmt.println(\"eq\")\n\
             }\n\
           }\n\
           f(map[string]int{})\n\
         }\n",
        "HYB-E2006",
    );
    assert_rejects(
        "fn main() {\n\
           f := fn() {\n\
             s := []int{1, 2}\n\
             if s == s {\n\
               fmt.println(\"eq\")\n\
             }\n\
           }\n\
           f()\n\
         }\n",
        "HYB-E2006",
    );

    // Positive control: closure parameter `m int` shadowing outer `m map[string]int`
    // must NOT falsely reject `m == 1` with HYB-E2006.
    assert_accepts(
        "fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           fmt.println(m[\"k\"])\n\
           f := fn(m int) {\n\
             if m == 1 {\n\
               fmt.println(m)\n\
             }\n\
           }\n\
           f(1)\n\
         }\n",
    );

    // Array length: non-int file `const`, shadowed file `const`, negative local `const`,
    // and non-int local `const` (`bool` / `float`) reject with HYB-E6004.
    assert_rejects(
        "const s_len = \"not_an_int\"\n\
         fn main() {\n\
           var a [s_len]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
    );
    assert_rejects(
        "const n_len = 4\n\
         fn main() {\n\
           n_len := 4\n\
           var a [n_len]int\n\
           fmt.println(\"{n_len} {a}\")\n\
         }\n",
        "HYB-E6004",
    );
    assert_rejects(
        "fn main() {\n\
           const neg_n = -1\n\
           var a [neg_n]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
    );
    assert_rejects(
        "fn main() {\n\
           const base_n = 2\n\
           const diff_n = base_n - 5\n\
           var a [diff_n]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
    );
    assert_rejects_at_line(
        "fn main() {\n\
           const bool_n = true\n\
           var a [bool_n]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
        3,
    );
    assert_rejects_at_line(
        "fn main() {\n\
           const not_true_n = !true\n\
           var a [not_true_n]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
        3,
    );
    assert_rejects_at_line(
        "fn main() {\n\
           const eq_n = 1 == 2\n\
           var a [eq_n]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
        3,
    );
    assert_rejects_at_line(
        "const file_not_true = !true\n\
         fn main() {\n\
           var a [file_not_true]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
        3,
    );
    assert_rejects_at_line(
        "fn main() {\n\
           const float_n = 1.5\n\
           var a [float_n]int\n\
           fmt.println(a)\n\
         }\n",
        "HYB-E6004",
        3,
    );

    // Positive control: function-local integer `const local_n = 4`, `len("abcd")`,
    // and integer conversions `int(4)` are valid Go array lengths.
    assert_accepts(
        "fn main() {\n\
           const local_n = 4\n\
           const str_len_n = len(\"abcd\")\n\
           const conv_n = int(4)\n\
           var a [local_n]int\n\
           var b [str_len_n]int\n\
           var c [conv_n]int\n\
           fmt.println(\"{a} {b} {c}\")\n\
         }\n",
    );
}

// ---------------------------------------------------------------------------
// 6. HYB-E1001 (struct type parameter leak & match pattern identifiers)
// ---------------------------------------------------------------------------

#[test]
fn struct_type_param_leak_and_bare_match_pattern_reject_e1001() {
    // Struct type parameter `t` must not leak into unrelated functions as a value name.
    assert_rejects_at_line(
        "struct container[t] { item int }\n\
         fn main() {\n\
           c := container[int]{item: 1}\n\
           x := t + c.item\n\
           fmt.println(x)\n\
         }\n",
        "HYB-E1001",
        4,
    );

    // Bare undeclared identifier in a `match` arm pattern must reject with HYB-E1001.
    assert_rejects_at_line(
        "enum color {\n\
           red\n\
           green\n\
         }\n\
         fn main() {\n\
           var c color = red\n\
           match c {\n\
             red: fmt.println(\"r\")\n\
             ghost_variant: fmt.println(\"g\")\n\
             _: fmt.println(\"other\")\n\
           }\n\
         }\n",
        "HYB-E1001",
        9,
    );
}
