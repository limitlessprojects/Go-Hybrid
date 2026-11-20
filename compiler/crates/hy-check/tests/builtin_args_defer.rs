//! Regression for issue #187: builtin argument validation (HYB-E2005) and
//! defer-non-call (HYB-E2001). Accepted programs used to emit Go that fails
//! `go build` / `go vet`.

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

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_lacks(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        !cs.iter().any(|c| c == code),
        "did not expect {}, got {:?}",
        code,
        cs
    );
}

#[test]
fn make_negative_size_rejected() {
    let r = check("fn main() {\n    c := make(chan int, -1)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_string_size_rejected() {
    let r = check("fn main() {\n    c := make(chan int, \"x\")\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_float_size_rejected() {
    let r = check("fn main() {\n    c := make(chan int, 2.5)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_slice_negative_size_rejected() {
    let r = check("fn main() {\n    s := make([]int, -1)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_variable_size_ok() {
    let r = check("fn main() {\n    n := 1\n    c := make(chan int, n)\n    close(c)\n}\n");
    assert_lacks(&r, "HYB-E2005");
    assert_lacks(&r, "HYB-E2001");
}

#[test]
fn make_slice_len_cap_accepted() {
    // `make([]T, len, cap)` is valid Go (issue #353).
    let r = check("fn main() {\n    s := make([]int, 3, 10)\n    s[0] = 1\n}\n");
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn make_slice_len_cap_variables_accepted() {
    let r = check(
        "fn main() {\n    n := 3\n    m := 10\n    s := make([]int, n, m)\n    s[0] = 1\n}\n",
    );
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn make_chan_three_args_rejected() {
    // The 3-argument form is slice-only: the same arity on a channel stays E2005.
    let r = check("fn main() {\n    c := make(chan int, 1, 2)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_slice_four_args_rejected() {
    let r = check("fn main() {\n    s := make([]int, 1, 2, 3)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_slice_negative_cap_rejected() {
    let r = check("fn main() {\n    s := make([]int, 1, -2)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn close_literal_rejected() {
    let r = check("fn main() {\n    close(5)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn len_int_literal_rejected() {
    let r = check("fn main() {\n    fmt.println(len(5))\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn cap_string_literal_rejected() {
    let r = check("fn main() {\n    fmt.println(cap(\"x\"))\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn len_string_literal_ok() {
    let r = check("fn main() {\n    fmt.println(len(\"abc\"))\n}\n");
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn delete_arity_one_rejected() {
    let r = check("fn main() {\n    m := map[string]int{\"a\": 1}\n    delete(m)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn delete_arity_two_ok() {
    let r = check("fn main() {\n    m := map[string]int{\"a\": 1}\n    delete(m, \"a\")\n}\n");
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn nested_place_builtin_args_rejected_e2005() {
    let r = check(
        "fn main() {\n    m := map[string][]int{\"a\": []int{1}}\n    m[string(make([]byte, -1))][0] = 1\n}\n",
    );
    assert_has(&r, "HYB-E2005");
}

#[test]
fn defer_ident_send_fusion_is_non_call() {
    // Parser yields Defer{Ident(ch)} then a following receive — not a Call.
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    defer ch <- 1\n    fmt.println(\"ok\")\n}\n",
    );
    assert_has(&r, "HYB-E2001");
}

#[test]
fn defer_receive_is_non_call() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    defer <-ch\n    fmt.println(\"ok\")\n}\n",
    );
    assert_has(&r, "HYB-E2001");
}

#[test]
fn defer_close_call_ok() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    defer close(ch)\n    fmt.println(\"ok\")\n}\n",
    );
    assert_lacks(&r, "HYB-E2001");
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn closure_body_builtin_args_checked() {
    let r = check(
        "fn main() {\n    f := fn() {\n        fmt.println(len(5))\n        fmt.println(cap(\"x\"))\n        close(5)\n    }\n    f()\n}\n",
    );
    assert_has(&r, "HYB-E2005");
}

#[test]
fn place_index_and_field_base_builtin_args_checked() {
    let r = check(
        "fn main() {\n    s := []int{1, 2}\n    s[len(5)] = 1\n}\n",
    );
    assert_has(&r, "HYB-E2005");
}

#[test]
fn bare_chan_literal_value_position_rejected() {
    let r = check("fn main() {\n    c := chan int\n    fmt.println(c)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_slice_one_arg_rejected() {
    let r = check("fn main() {\n    s := make([]int)\n    fmt.println(len(s))\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_slice_const_len_exceeds_cap_rejected() {
    let r = check("fn main() {\n    s := make([]int, 10, 5)\n    fmt.println(len(s))\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_negative_const_expr_size_rejected() {
    let r = check("fn main() {\n    s := make([]int, 1 - 5)\n    fmt.println(len(s))\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_underscored_and_hex_size_accepted() {
    let r = check("fn main() {\n    s := make([]int, 1_024, 0x800)\n    s[0] = 1\n}\n");
    assert_lacks(&r, "HYB-E2005");
}

#[test]
fn make_non_makeable_type_rejected() {
    let r = check("fn main() {\n    x := make(int, 1)\n    fmt.println(x)\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn make_type_alias_slice_accepted_and_validated() {
    let ok = check("type int_slice = []int\nfn main() {\n    s := make(int_slice, 2, 4)\n    s[0] = 1\n}\n");
    assert_lacks(&ok, "HYB-E2005");

    let bad_one_arg = check("type int_slice = []int\nfn main() {\n    s := make(int_slice)\n    fmt.println(len(s))\n}\n");
    assert_has(&bad_one_arg, "HYB-E2005");
}

#[test]
fn copy_and_clear_builtin_args_validated() {
    let bad_copy_arity = check("fn main() {\n    s := []int{1}\n    copy(s)\n}\n");
    assert_has(&bad_copy_arity, "HYB-E2005");

    let bad_copy_dst = check("fn main() {\n    s := []int{1}\n    copy(\"abc\", s)\n}\n");
    assert_has(&bad_copy_dst, "HYB-E2005");

    let bad_clear_lit = check("fn main() {\n    clear(5)\n}\n");
    assert_has(&bad_clear_lit, "HYB-E2005");
}

#[test]
fn parenthesized_and_aliased_new_preserves_sendability_and_unmovable_checks() {
    let closure_paren = check(
        "struct holder {\n    f fn()\n}\nfn worker(p *holder) {\n    p.f()\n}\nfn main() {\n    p := new((holder))\n    task worker(move p)\n}\n",
    );
    assert_has(&closure_paren, "HYB-E3001");

    let closure_alias = check(
        "struct holder {\n    f fn()\n}\ntype h_alias = holder\nfn worker(p *h_alias) {\n    p.f()\n}\nfn main() {\n    p := new(h_alias)\n    task worker(move p)\n}\n",
    );
    assert_has(&closure_alias, "HYB-E3001");

    let closure_generic = check(
        "struct box[t] {\n    v t\n    f fn()\n}\nfn worker(p *box[int]) {\n    p.f()\n}\nfn main() {\n    p := new(box[int])\n    task worker(move p)\n}\n",
    );
    assert_has(&closure_generic, "HYB-E3001");

    let unmovable_wg = check(
        "struct wg_holder {\n    wg sync.wait_group\n}\nfn worker(p *wg_holder) {\n    fmt.println(p)\n}\nfn main() {\n    p1 := new((wg_holder))\n    task worker(move p1)\n    p2 := new(sync.wait_group)\n    p2.done()\n}\n",
    );
    assert_has(&unmovable_wg, "HYB-E3001");
}

#[test]
fn parenthesized_and_aliased_make_preserves_share_and_closure_checks() {
    let paren_make_closure = check(
        "struct holder {\n    f fn()\n}\ntype holder_slice = []holder\nfn worker(s holder_slice) {\n    fmt.println(len(s))\n}\nfn main() {\n    s := make((holder_slice), 1)\n    p := &s\n    fmt.println(p)\n    task worker(move s)\n}\n",
    );
    assert_has(&paren_make_closure, "HYB-E3001");

    let alias_make_copy_share = check(
        "type int_slice = []int\nfn worker(s int_slice) {\n    s[0] = 2\n}\nfn main() {\n    s1 := make((int_slice), 1)\n    s2 := s1\n    task worker(move s2)\n    s1[0] = 1\n}\n",
    );
    assert_has(&alias_make_copy_share, "HYB-E3003");
}

#[test]
fn make_array_lit_rejected_hyb_e2005() {
    let r1 = check("fn main() {\n    _ = make([4]int{}, 4)\n}\n");
    assert_has(&r1, "HYB-E2005");

    let r2 = check("fn main() {\n    _ = make(([4]int{}), 4)\n}\n");
    assert_has(&r2, "HYB-E2005");
}

#[test]
fn issue_2102_array_len_builtin_args_rejected_hyb_e2005() {
    // 1. var a [len(1)]int
    let r1 = check("fn main() {\n    var a [len(1)]int\n    _ = a\n}\n");
    assert_has(&r1, "HYB-E2005");

    // 2. b := [cap("hi") + 1]int{0}
    let r2 = check("fn main() {\n    b := [cap(\"hi\") + 1]int{0}\n    _ = b\n}\n");
    assert_has(&r2, "HYB-E2005");

    // 3. type alias: type arr_alias = [len()]int
    let r3 = check("type arr_alias = [len()]int\nfn main() {}\n");
    assert_has(&r3, "HYB-E2005");

    // 4. struct field: struct my_struct { a [min()]int }
    let r4 = check("struct my_struct {\n    a [min()]int\n}\nfn main() {}\n");
    assert_has(&r4, "HYB-E2005");

    // 5. valid array length should pass without HYB-E2005
    let r_ok = check("fn main() {\n    var a [4]int\n    _ = a\n}\n");
    assert_lacks(&r_ok, "HYB-E2005");
}


