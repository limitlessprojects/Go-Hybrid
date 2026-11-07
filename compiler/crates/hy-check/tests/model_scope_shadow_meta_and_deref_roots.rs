use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|e| panic!("parse failed: {:?}", e));
    check_file(&ast)
}

#[test]
fn inner_shadow_preserves_outer_borrow_bindings_for_return_loan_escape_e3104() {
    let r_named = check(
        "type int_ptr = *int\n\
         fn leak_named() (out int_ptr) {\n\
           x := 1\n\
           out = &x\n\
           if true {\n\
             out := 42\n\
             fmt.println(out)\n\
           }\n\
           return\n\
         }",
    );
    assert_eq!(r_named.verdict, Verdict::Reject);
    assert!(
        r_named.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 for bare return of named result after inner shadow, got {:?}",
        r_named.diagnostics
    );

    let r_var = check(
        "type int_ptr = *int\n\
         fn leak_var() int_ptr {\n\
           x := 1\n\
           p := &x\n\
           if true {\n\
             p := 42\n\
             fmt.println(p)\n\
           }\n\
           return p\n\
         }",
    );
    assert_eq!(r_var.verdict, Verdict::Reject);
    assert!(
        r_var.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 for returning outer borrow binding after inner shadow, got {:?}",
        r_var.diagnostics
    );
}

#[test]
fn inner_borrow_shadow_does_not_leak_to_outer_non_borrowed_pointer() {
    let r = check(
        "struct item { n int }\n\
         fn safe_return(p *item) *item {\n\
           if true {\n\
             x := item{n: 1}\n\
             p := &x\n\
             fmt.println(p.n)\n\
           }\n\
           return p\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}

#[test]
fn inner_shadow_preserves_outer_closure_mutates_fnlits_and_defs() {
    let r_a3 = check(
        "struct item { v int }\n\
         fn main() {\n\
           m := map[string]item{\"a\": item{v: 1}}\n\
           f := fn() { m[\"b\"] = item{v: 2} }\n\
           if true {\n\
             f := 1\n\
             f = 2\n\
             fmt.println(f)\n\
           }\n\
           p := &m[\"a\"]\n\
           f()\n\
           fmt.println(p.v)\n\
         }",
    );
    assert_eq!(r_a3.verdict, Verdict::Reject);
    assert!(
        r_a3.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 when calling outer mutating closure after inner shadow, got {:?}",
        r_a3.diagnostics
    );

    let r_origin = check(
        "fn run(s []int) {\n\
           f := fn(xs []int) []int { return xs }\n\
           if true {\n\
             f := fn(xs []int) []int { return []int{1} }\n\
             _ = f(s)\n\
           }\n\
           y := f(s)\n\
           ch := make(chan []int, 1)\n\
           ch <- move y\n\
         }",
    );
    assert_eq!(r_origin.verdict, Verdict::Reject);
    assert!(
        r_origin.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving result of outer parameter-returning closure after inner shadow, got {:?}",
        r_origin.diagnostics
    );

    let r_param_store = check(
        "struct box { s []int }\n\
         fn main() {\n\
           f := fn(b *box, xs []int) { b.s = xs }\n\
           if true {\n\
             f := fn(b *box, xs []int) {}\n\
             var tmp = box{s: []int{0}}\n\
             f(&tmp, []int{1})\n\
           }\n\
           var b = box{s: []int{0}}\n\
           xs := []int{1, 2}\n\
           f(&b, xs)\n\
           ch := make(chan []int, 1)\n\
           ch <- move xs\n\
           fmt.println(b.s)\n\
         }",
    );
    assert_eq!(r_param_store.verdict, Verdict::Reject);
    assert!(
        r_param_store.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when outer parameter-storing closure is called after inner shadow, got {:?}",
        r_param_store.diagnostics
    );
}

#[test]
fn some_wrapped_and_projected_addr_of_roots_enforce_freeze_and_const_write() {
    let r_some_freeze = check(
        "struct item { n int }\n\
         type item_ptr = *item\n\
         struct holder { opt option[item_ptr] }\n\
         fn main() {\n\
           it := item{n: 1}\n\
           h := holder{opt: some(&it)}\n\
           ch := make(chan item, 1)\n\
           ch <- move it\n\
           _ = h.opt\n\
         }",
    );
    assert_eq!(r_some_freeze.verdict, Verdict::Reject);
    assert!(
        r_some_freeze.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when reading holder(some(&it)) after moving it, got {:?}",
        r_some_freeze.diagnostics
    );

    let r_some_const = check(
        "struct item { n int }\n\
         type item_ptr = *item\n\
         fn mut_opt(o option[item_ptr]) {\n\
           p := o.or(&item{n: 0})\n\
           p.n = 9\n\
         }\n\
         fn main() {\n\
           const c = item{n: 1}\n\
           mut_opt(some(&c))\n\
         }",
    );
    assert_eq!(r_some_const.verdict, Verdict::Reject);
    assert!(
        r_some_const.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when passing some(&c) of const struct to mutating fn, got {:?}",
        r_some_const.diagnostics
    );

    let r_proj_const = check(
        "struct item { n int }\n\
         struct box { items []item }\n\
         fn mut_ptr(p *item) { p.n = 9 }\n\
         fn main() {\n\
           const s = []item{item{n: 1}}\n\
           mut_ptr(&((box{items: s}).items[0]))\n\
         }",
    );
    assert_eq!(r_proj_const.verdict, Verdict::Reject);
    assert!(
        r_proj_const.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when passing projected addr-of of const slice to mutating fn, got {:?}",
        r_proj_const.diagnostics
    );
}
