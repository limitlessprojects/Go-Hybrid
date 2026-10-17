//! Regression tests for nested assignment-target `Place` call/closure
//! traversal (`walk::own_calls`, `walk::own_fnlits`,
//! `builtins::check_place_builtin_args`), parenthesized callee resolution,
//! `ForKind::Iterate` interprocedural mutation scanning, and A3 map-slot
//! loan invalidation (`HYB-E3102`) across Hybrid methods, closure parameters,
//! and inline closure invocations.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_code(r: &hy_check::CheckResult, want: &str, what: &str) {
    let cs = codes(r);
    assert!(
        cs.iter().any(|c| c == want),
        "expected {} ({}), got {:?}",
        want,
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nested_place_copy_links_may_share_and_rejects_post_move_write_e3003() {
    let r = check(
        "struct box { n int }\n\
         fn worker(d []map[string]int, done chan bool) {\n\
             d[0][\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var src []map[string]int = []map[string]int{map[string]int{\"k\": 0}}\n\
             var dst []map[string]int = []map[string]int{map[string]int{\"k\": 0}}\n\
             boxes := []box{box{n: 0}, box{n: 0}}\n\
             boxes[copy(dst, src)].n = 42\n\
             done := make(chan bool, 1)\n\
             task worker(move dst, done)\n\
             src[0][\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert_code(
        &r,
        "HYB-E3003",
        "copy(dst, src) inside nested assignment place",
    );
}

#[test]
fn nested_place_calls_and_closures_enforce_const_deep_immutability_e3006() {
    let r_call = check(
        "struct box { n int }\n\
         const shared = map[string]int{\"a\": 1}\n\
         fn corrupt(m map[string]int) int {\n\
             m[\"a\"] = 99\n\
             return 0\n\
         }\n\
         fn main() {\n\
             boxes := []box{box{n: 0}}\n\
             boxes[corrupt(shared)].n = 42\n\
         }\n",
    );
    assert_code(
        &r_call,
        "HYB-E3006",
        "interprocedural const write in nested place",
    );

    let r_closure = check(
        "const shared = map[string]int{\"a\": 1}\n\
         fn main() {\n\
             arr := []int{0}\n\
             arr[fn() int {\n\
                 shared[\"a\"] = 99\n\
                 return 0\n\
             }()] = 10\n\
         }\n",
    );
    assert_code(
        &r_closure,
        "HYB-E3006",
        "closure const write in assignment target place",
    );
}

#[test]
fn nested_place_call_and_builtin_checks_enforce_a3_and_e2005() {
    let r_a3 = check(
        "struct svc { n int }\n\
         struct box { n int }\n\
         fn del_and_idx(m map[string]svc) int {\n\
             delete(m, \"k\")\n\
             return 0\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             boxes := []box{box{n: 0}}\n\
             slot := &m[\"k\"]\n\
             boxes[del_and_idx(m)].n = 99\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_a3,
        "HYB-E3102",
        "A3 map mutation inside nested assignment place",
    );

    let r_builtin = check(
        "struct box { n int }\n\
         fn main() {\n\
             boxes := []box{box{n: 0}}\n\
             boxes[len(1, 2)].n = 3\n\
         }\n",
    );
    assert_code(
        &r_builtin,
        "HYB-E2005",
        "builtin arity error inside nested assignment place",
    );
}

#[test]
fn parenthesized_callees_preserve_copy_and_delete_soundness() {
    let r_copy = check(
        "fn worker(d []map[string]int, done chan bool) {\n\
             d[0][\"k\"] = 1\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var src []map[string]int = []map[string]int{map[string]int{\"k\": 0}}\n\
             var dst []map[string]int = []map[string]int{map[string]int{\"k\": 0}}\n\
             if true {\n\
                 (copy)(dst, src)\n\
             }\n\
             done := make(chan bool, 1)\n\
             task worker(move dst, done)\n\
             src[0][\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert_code(&r_copy, "HYB-E3003", "parenthesized (copy)(dst, src)");

    let r_del = check(
        "struct svc { n int }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             if true {\n\
                 (delete)(m, \"k\")\n\
             }\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_del,
        "HYB-E3102",
        "parenthesized (delete)(m, \"k\") in A3 window",
    );
}

#[test]
fn a3_map_slot_loan_rejects_inline_closures_closure_params_and_methods_e3102() {
    let r_iife = check(
        "struct svc { n int }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             fn() {\n\
                 delete(m, \"k\")\n\
             }()\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_iife,
        "HYB-E3102",
        "inline closure mutating captured map in A3 window",
    );

    let r_closure_param = check(
        "struct svc { n int }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             f := fn(x map[string]svc) {\n\
                 delete(x, \"k\")\n\
             }\n\
             f(m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_closure_param,
        "HYB-E3102",
        "closure mutating map parameter in A3 window",
    );

    let r_method_arg = check(
        "struct svc { n int }\n\
         struct mutator { tag int }\n\
         fn (u mutator) drop_k(m map[string]svc) {\n\
             _ = u\n\
             delete(m, \"k\")\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             u := mutator{tag: 1}\n\
             slot := &m[\"k\"]\n\
             u.drop_k(m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_method_arg,
        "HYB-E3102",
        "method mutating map argument in A3 window",
    );

    let r_method_recv = check(
        "struct svc { n int }\n\
         struct engine { services map[string]svc }\n\
         fn (e engine) reset() {\n\
             delete(e.services, \"k\")\n\
         }\n\
         fn main() {\n\
             e := engine{services: map[string]svc{\"k\": svc{n: 1}}}\n\
             slot := &e.services[\"k\"]\n\
             e.reset()\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_method_recv,
        "HYB-E3102",
        "method mutating receiver map in A3 window",
    );
}

#[test]
fn for_iterate_header_and_expr_closures_propagate_interproc_mutations() {
    let r_const_iter = check(
        "const shared = map[string]int{\"a\": 1}\n\
         fn corrupt_and_ret(m map[string]int) []int {\n\
             m[\"a\"] = 99\n\
             return []int{1}\n\
         }\n\
         fn helper(m map[string]int) {\n\
             for v in corrupt_and_ret(m) {\n\
                 _ = v\n\
             }\n\
         }\n\
         fn main() {\n\
             helper(shared)\n\
         }\n",
    );
    assert_code(
        &r_const_iter,
        "HYB-E3006",
        "ForKind::Iterate header call mutating const param",
    );

    let r_const_iife = check(
        "const shared = map[string]int{\"a\": 1}\n\
         fn helper(m map[string]int) {\n\
             fn() {\n\
                 m[\"a\"] = 99\n\
             }()\n\
         }\n\
         fn main() {\n\
             helper(shared)\n\
         }\n",
    );
    assert_code(
        &r_const_iife,
        "HYB-E3006",
        "Stmt::Expr closure mutating const param",
    );
}

#[test]
fn positive_controls_read_only_closures_methods_and_pure_pkg_parens_accept() {
    let r = check(
        "import \"fmt\"\n\
         struct svc { n int }\n\
         struct engine { services map[string]svc }\n\
         fn (e engine) count() int {\n\
             return len(e.services)\n\
         }\n\
         fn (e engine) count_map(m map[string]svc) int {\n\
             _ = e\n\
             return len(m)\n\
         }\n\
         fn log_map(m map[string]svc) {\n\
             (fmt).println(len(m))\n\
         }\n\
         fn main() {\n\
             e := engine{services: map[string]svc{\"k\": svc{n: 1}}}\n\
             slot := &e.services[\"k\"]\n\
             ro := fn(x map[string]svc) int {\n\
                 return len(x)\n\
             }\n\
             log_map(e.services)\n\
             n := ro(e.services) + e.count() + e.count_map(e.services)\n\
             slot.n = n\n\
         }\n",
    );
    assert!(
        codes(&r).is_empty(),
        "read-only closures, methods, and parenthesized pure package calls must accept: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn wrapped_closures_and_computed_callees_fail_closed_in_a3_and_const_checks() {
    let r_wrapped = check(
        "struct svc { n int }\n\
         fn wrap(cb fn(map[string]svc) int) fn(map[string]svc) int {\n\
             _ = cb\n\
             return fn(x map[string]svc) int {\n\
                 delete(x, \"k\")\n\
                 return 0\n\
             }\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             f := wrap(fn(x map[string]svc) int {\n\
                 return len(x)\n\
             })\n\
             slot := &m[\"k\"]\n\
             _ = f(m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_wrapped,
        "HYB-E3102",
        "wrapped closure must not inherit inner fnlit read-only param set",
    );

    let r_slice_callee = check(
        "struct svc { n int }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             fs := []fn(map[string]svc){fn(x map[string]svc) {\n\
                 delete(x, \"k\")\n\
             }}\n\
             slot := &m[\"k\"]\n\
             fs[0](m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_slice_callee,
        "HYB-E3102",
        "slice-indexed closure call in A3 window",
    );

    let r_call_callee = check(
        "struct svc { n int }\n\
         fn getf() fn(map[string]svc) {\n\
             return fn(x map[string]svc) {\n\
                 delete(x, \"k\")\n\
             }\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             getf()(m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_call_callee,
        "HYB-E3102",
        "call-returned closure invocation in A3 window",
    );

    let r_const_computed = check(
        "const shared = map[string]int{\"a\": 1}\n\
         fn get_mutator() fn(map[string]int) {\n\
             return fn(x map[string]int) {\n\
                 x[\"a\"] = 99\n\
             }\n\
         }\n\
         fn main() {\n\
             get_mutator()(shared)\n\
         }\n",
    );
    assert_code(
        &r_const_computed,
        "HYB-E3006",
        "computed callee with const map argument",
    );
}

#[test]
fn pointer_receiver_methods_and_closure_rebinds_update_a3_state() {
    let r_ptr_recv = check(
        "struct svc { n int }\n\
         struct engine { services map[string]svc }\n\
         fn (e *engine) reset() {\n\
             delete(e.services, \"k\")\n\
         }\n\
         fn main() {\n\
             e := engine{services: map[string]svc{\"k\": svc{n: 1}}}\n\
             p := &e\n\
             slot := &e.services[\"k\"]\n\
             p.reset()\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_ptr_recv,
        "HYB-E3102",
        "pointer-receiver method mutating map in A3 window",
    );

    let r_rebind_to_wrapped = check(
        "struct svc { n int }\n\
         fn wrap(cb fn(map[string]svc) int) fn(map[string]svc) int {\n\
             return cb\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             f := fn(x map[string]svc) int {\n\
                 return len(x)\n\
             }\n\
             f = wrap(f)\n\
             slot := &m[\"k\"]\n\
             _ = f(m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_rebind_to_wrapped,
        "HYB-E3102",
        "rebinding read-only closure to unknown call result clears known param set",
    );
}

#[test]
fn method_name_disambiguation_and_for_iterate_single_diagnostic() {
    let r_disambig = check(
        "struct svc { n int }\n\
         struct writer { tag int }\n\
         struct reader { tag int }\n\
         fn (w writer) apply(m map[string]svc) int {\n\
             _ = w\n\
             delete(m, \"k\")\n\
             return 0\n\
         }\n\
         fn (r reader) apply(m map[string]svc) int {\n\
             _ = r\n\
             return len(m)\n\
         }\n\
         fn helper(r reader, m map[string]svc) int {\n\
             return r.apply(m)\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             r := reader{tag: 1}\n\
             slot := &m[\"k\"]\n\
             n := helper(r, m)\n\
             slot.n = n\n\
         }\n",
    );
    assert!(
        codes(&r_disambig).is_empty(),
        "helper calling read-only reader.apply must not collide with writer.apply: {:?}",
        r_disambig.diagnostics
    );
    assert_eq!(r_disambig.verdict, Verdict::Accept);

    let r_iter_single = check(
        "const shared = map[string]int{\"a\": 1}\n\
         fn corrupt_items(m map[string]int) []int {\n\
             m[\"a\"] = 99\n\
             return []int{1}\n\
         }\n\
         fn main() {\n\
             for v in corrupt_items(shared) {\n\
                 _ = v\n\
             }\n\
         }\n",
    );
    assert_eq!(
        codes(&r_iter_single),
        vec!["HYB-E3006".to_string()],
        "for-iterate header call processed by loans::process_call must emit a single diagnostic"
    );

    let r_call_init_writer = check(
        "struct svc { n int }\n\
         struct writer { tag int }\n\
         struct reader { tag int }\n\
         fn make_writer() writer {\n\
             return writer{tag: 1}\n\
         }\n\
         fn (w writer) apply(m map[string]svc) int {\n\
             _ = w\n\
             delete(m, \"k\")\n\
             return 0\n\
         }\n\
         fn (r reader) apply(m map[string]svc) int {\n\
             _ = r\n\
             return len(m)\n\
         }\n\
         fn helper(m map[string]svc) int {\n\
             r := make_writer()\n\
             return r.apply(m)\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             n := helper(m)\n\
             slot.n = n\n\
         }\n",
    );
    assert_code(
        &r_call_init_writer,
        "HYB-E3102",
        "call-initialized writer local named r must not inherit reader receiver type from another fn",
    );

    let r_same_var_name_diff_fns = check(
        "struct svc { n int }\n\
         struct writer { tag int }\n\
         struct reader { tag int }\n\
         fn (r writer) apply(m map[string]svc) int {\n\
             _ = r\n\
             delete(m, \"k\")\n\
             return 0\n\
         }\n\
         fn (r reader) apply(m map[string]svc) int {\n\
             _ = r\n\
             return len(m)\n\
         }\n\
         fn read_helper(r reader, m map[string]svc) int {\n\
             return r.apply(m)\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             rd := reader{tag: 1}\n\
             slot := &m[\"k\"]\n\
             n := read_helper(rd, m)\n\
             slot.n = n\n\
         }\n",
    );
    assert!(
        codes(&r_same_var_name_diff_fns).is_empty(),
        "lexical receiver type must disambiguate even when both methods name their receiver r: {:?}",
        r_same_var_name_diff_fns.diagnostics
    );
    assert_eq!(r_same_var_name_diff_fns.verdict, Verdict::Accept);
}

#[test]
fn closure_shadowing_and_branch_rebinds_preserve_lexical_soundness() {
    let r_shadow_wrapped = check(
        "struct svc { n int }\n\
         fn wrap(cb fn(map[string]svc) int) fn(map[string]svc) int {\n\
             _ = cb\n\
             return fn(x map[string]svc) int {\n\
                 delete(x, \"k\")\n\
                 return 0\n\
             }\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             f := fn(x map[string]svc) int {\n\
                 return len(x)\n\
             }\n\
             if true {\n\
                 f := wrap(f)\n\
                 slot := &m[\"k\"]\n\
                 _ = f(m)\n\
                 slot.n = 2\n\
             }\n\
         }\n",
    );
    assert_code(
        &r_shadow_wrapped,
        "HYB-E3102",
        "inner shadowed f := wrap(f) must not reuse outer read-only closure summary",
    );

    let r_branch_rebind = check(
        "struct svc { n int }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             f := fn(x map[string]svc) int {\n\
                 return len(x)\n\
             }\n\
             if true {\n\
                 f = fn(x map[string]svc) int {\n\
                     delete(x, \"k\")\n\
                     return 0\n\
                 }\n\
             }\n\
             slot := &m[\"k\"]\n\
             _ = f(m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_branch_rebind,
        "HYB-E3102",
        "branch rebind of outer closure to mutating closure must propagate out of block",
    );

    let r_shadow_restores_outer = check(
        "struct svc { n int }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             f := fn(x map[string]svc) int {\n\
                 return len(x)\n\
             }\n\
             if true {\n\
                 f := fn(x map[string]svc) int {\n\
                     delete(x, \"k\")\n\
                     return 0\n\
                 }\n\
                 _ = f\n\
             }\n\
             slot := &m[\"k\"]\n\
             n := f(m)\n\
             slot.n = n\n\
         }\n",
    );
    assert!(
        codes(&r_shadow_restores_outer).is_empty(),
        "outer read-only closure summary must be restored after inner shadowing block exits: {:?}",
        r_shadow_restores_outer.diagnostics
    );
    assert_eq!(r_shadow_restores_outer.verdict, Verdict::Accept);
}

#[test]
fn for_loop_var_and_closure_inner_block_shadowing_do_not_misclassify_receiver_struct() {
    let r_for_shadow_fn = check(
        "struct svc { n int }\n\
         struct reader { tag int }\n\
         struct writer { tag int }\n\
         fn (r reader) apply(m map[string]svc) int {\n\
             return len(m) + r.tag\n\
         }\n\
         fn (w writer) apply(m map[string]svc) int {\n\
             delete(m, \"k\")\n\
             return w.tag\n\
         }\n\
         fn helper(r reader, ws []writer, m map[string]svc) int {\n\
             for _, r in ws {\n\
                 _ = r.apply(m)\n\
             }\n\
             return 0\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             ws := []writer{writer{tag: 1}}\n\
             slot := &m[\"k\"]\n\
             _ = helper(reader{tag: 0}, ws, m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_for_shadow_fn,
        "HYB-E3102",
        "for-loop variable shadowing outer reader parameter with writer must reject with E3102",
    );

    let r_closure_inner_block_shadow = check(
        "struct svc { n int }\n\
         struct reader { tag int }\n\
         struct writer { tag int }\n\
         fn (r reader) apply(m map[string]svc) int {\n\
             return len(m) + r.tag\n\
         }\n\
         fn (w writer) apply(m map[string]svc) int {\n\
             delete(m, \"k\")\n\
             return w.tag\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             f := fn() {\n\
                 w := writer{tag: 1}\n\
                 if true {\n\
                     w := reader{tag: 0}\n\
                     _ = w\n\
                 }\n\
                 _ = w.apply(m)\n\
             }\n\
             slot := &m[\"k\"]\n\
             f()\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_closure_inner_block_shadow,
        "HYB-E3102",
        "inner block shadow w := reader{} inside closure must not leak past block exit",
    );
}

#[test]
fn temporary_receiver_holding_loaned_map_and_inner_alias_closure_rebind_reject() {
    let r_temp_recv = check(
        "struct svc { n int }\n\
         struct engine { services map[string]svc }\n\
         fn (e *engine) reset() {\n\
             delete(e.services, \"k\")\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             engine{services: m}.reset()\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_temp_recv,
        "HYB-E3102",
        "temporary composite receiver holding loaned map must reject mutating method call with E3102",
    );

    let r_inner_alias_closure = check(
        "struct svc { n int }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             f := fn() {}\n\
             if true {\n\
                 tmp := m\n\
                 f = fn() {\n\
                     delete(tmp, \"k\")\n\
                 }\n\
             }\n\
             f()\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_inner_alias_closure,
        "HYB-E3102",
        "closure rebound in inner scope to mutate block-local alias of outer map must reject after scope pop",
    );
}

#[test]
fn non_struct_lexical_binding_shadowing_top_level_fn_rejects() {
    let r_factory_shadow_a3 = check(
        "struct svc { n int }\n\
         struct reader { tag int }\n\
         struct writer { tag int }\n\
         fn (r reader) apply(m map[string]svc) int {\n\
             return len(m) + r.tag\n\
         }\n\
         fn (w writer) apply(m map[string]svc) int {\n\
             delete(m, \"k\")\n\
             return w.tag\n\
         }\n\
         fn factory() reader {\n\
             return reader{tag: 0}\n\
         }\n\
         fn helper(factory fn() writer, m map[string]svc) int {\n\
             r := factory()\n\
             return r.apply(m)\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             _ = helper(fn() writer { return writer{tag: 1} }, m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_factory_shadow_a3,
        "HYB-E3102",
        "closure parameter factory shadowing top-level fn factory() reader must not resolve r := factory() to reader",
    );

    let r_direct_fn_param_shadow_a3 = check(
        "struct svc { n int }\n\
         fn op(m map[string]svc) int {\n\
             return len(m)\n\
         }\n\
         fn helper(op fn(map[string]svc) int, m map[string]svc) int {\n\
             return op(m)\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             _ = helper(fn(x map[string]svc) int { delete(x, \"k\")\n return 0 }, m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_direct_fn_param_shadow_a3,
        "HYB-E3102",
        "parameter op shadowing top-level read-only fn op must fail closed on op(m)",
    );

    let r_factory_shadow_const = check(
        "struct svc { n int }\n\
         struct reader { tag int }\n\
         struct writer { tag int }\n\
         fn (r reader) apply(m map[string]svc) int {\n\
             return len(m) + r.tag\n\
         }\n\
         fn (w writer) apply(m map[string]svc) int {\n\
             delete(m, \"k\")\n\
             return w.tag\n\
         }\n\
         fn factory() reader {\n\
             return reader{tag: 0}\n\
         }\n\
         fn helper(factory fn() writer, m map[string]svc) int {\n\
             r := factory()\n\
             return r.apply(m)\n\
         }\n\
         fn main() {\n\
             const m = map[string]svc{\"k\": svc{n: 1}}\n\
             _ = helper(fn() writer { return writer{tag: 1} }, m)\n\
         }\n",
    );
    assert_code(
        &r_factory_shadow_const,
        "HYB-E3006",
        "closure parameter factory shadowing top-level fn factory() reader must reject const map write with E3006",
    );

    let r_shadowed_builtin_a3 = check(
        "struct svc { n int }\n\
         fn helper(println fn(map[string]svc) int, m map[string]svc) int {\n\
             return println(m)\n\
         }\n\
         fn main() {\n\
             m := map[string]svc{\"k\": svc{n: 1}}\n\
             slot := &m[\"k\"]\n\
             _ = helper(fn(x map[string]svc) int { delete(x, \"k\")\n return 0 }, m)\n\
             slot.n = 2\n\
         }\n",
    );
    assert_code(
        &r_shadowed_builtin_a3,
        "HYB-E3102",
        "parameter println shadowing builtin println must not be treated as pure in A3",
    );

    let r_shadowed_builtin_const = check(
        "struct svc { n int }\n\
         fn helper(println fn(map[string]svc) int, m map[string]svc) int {\n\
             return println(m)\n\
         }\n\
         fn main() {\n\
             const m = map[string]svc{\"k\": svc{n: 1}}\n\
             _ = helper(fn(x map[string]svc) int { delete(x, \"k\")\n return 0 }, m)\n\
         }\n",
    );
    assert_code(
        &r_shadowed_builtin_const,
        "HYB-E3006",
        "parameter println shadowing builtin println must not bypass const-write check with E3006",
    );
}
