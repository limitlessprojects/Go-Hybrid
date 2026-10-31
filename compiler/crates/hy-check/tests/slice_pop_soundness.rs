use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

// ---------------------------------------------------------------------------
// Hole 1: `.pop()` return TypeShape / TypeExpr / OpKind preservation
// ---------------------------------------------------------------------------

#[test]
fn slice_pop_preserves_reference_element_shape_and_freezes_moved_peer_e3003() {
    let r = check(
        "fn worker(m map[string]int, done chan bool) {\n\
           m[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[string]int{\"k\": 0}\n\
           stack := []map[string]int{m}\n\
           m2 := stack.pop()\n\
           task worker(move m, done)\n\
           m2[\"k\"] = 2\n\
           <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when mutating m2 after moving m, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_pop_freezes_source_slice_when_popped_map_is_moved_e3003() {
    let r = check(
        "fn worker(m map[string]int, done chan bool) {\n\
           m[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[string]int{\"k\": 0}\n\
           stack := []map[string]int{m, m}\n\
           m2 := stack.pop()\n\
           task worker(move m2, done)\n\
           stack[0][\"k\"] = 2\n\
           <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when mutating stack[0] after moving popped element m2, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_pop_from_param_propagates_non_owned_origin_e3004() {
    let r = check(
        "fn worker(m map[string]int, done chan bool) {\n\
           m[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn bad(stack []map[string]int, done chan bool) {\n\
           m2 := stack.pop()\n\
           task worker(move m2, done)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving element popped from parameter slice, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_pop_primitive_element_is_sendable_without_false_e3005() {
    let r = check(
        "fn worker(v int, done chan int) {\n\
           done <- v\n\
         }\n\
         fn main() {\n\
           ch := make(chan int, 2)\n\
           stack := []int{10, 20}\n\
           v := stack.pop()\n\
           ch <- v\n\
           task worker(v, ch)\n\
           <-ch\n\
           <-ch\n\
         }\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "primitive element popped from []int must be sendable without HYB-E3005: {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Hole 2: T22 (`HYB-E3006`) deep immutability on `.pop()`
// ---------------------------------------------------------------------------

#[test]
fn file_const_slice_pop_rejected_with_e3006() {
    let r = check(
        "const global_stack = []int{10, 20, 30}\n\
         fn main() {\n\
           _ = global_stack.pop()\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 on global_stack.pop(), got: {:?}",
        r.diagnostics
    );
}

#[test]
fn local_const_and_interprocedural_fn_slice_pop_rejected_with_e3006() {
    let r1 = check(
        "fn main() {\n\
           const xs = []int{1, 2, 3}\n\
           _ = xs.pop()\n\
         }\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3006"));

    let r2 = check(
        "const global_stack = []int{1, 2, 3}\n\
         fn pop_one(xs []int) int {\n\
           return xs.pop()\n\
         }\n\
         fn main() {\n\
           _ = pop_one(global_stack)\n\
         }\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3006"));
}

#[test]
fn alias_and_const_struct_field_slice_pop_rejected_with_e3006() {
    let r1 = check(
        "const global_stack = []int{10, 20}\n\
         fn main() {\n\
           alias := global_stack\n\
           _ = alias.pop()\n\
         }\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3006"));

    let r2 = check(
        "struct holder {\n\
           xs []int\n\
         }\n\
         fn main() {\n\
           const h = holder{xs: []int{1, 2}}\n\
           _ = h.xs.pop()\n\
         }\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3006"));
}

#[test]
fn closure_and_interprocedural_slice_pop_on_const_rejected_with_e3006() {
    let r1 = check(
        "const global_stack = []int{10, 20}\n\
         fn main() {\n\
           f := fn() int {\n\
             return global_stack.pop()\n\
           }\n\
           _ = f()\n\
         }\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3006"));

    let r2 = check(
        "struct custom {}\n\
         fn (c custom) pop() int { return 0 }\n\
         struct holder {\n\
           xs []int\n\
         }\n\
         fn shrink(h *holder) int {\n\
           return h.xs.pop()\n\
         }\n\
         fn main() {\n\
           const h = holder{xs: []int{1, 2}}\n\
           _ = shrink(&h)\n\
         }\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3006"));
}

// ---------------------------------------------------------------------------
// Hole 3: `.pop()` operand kind, addressability, and arity guards
// ---------------------------------------------------------------------------

#[test]
fn slice_pop_with_arguments_rejected_with_e2005() {
    let r = check(
        "fn main() {\n\
           s := []int{1, 2, 3}\n\
           _ = s.pop(1)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E2005"),
        "expected HYB-E2005 on s.pop(1), got: {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_pop_on_non_addressable_or_non_slice_rejected_with_e1002() {
    let non_addr_lit = check(
        "fn main() {\n\
           _ = ([]int{1, 2}).pop()\n\
         }\n",
    );
    assert_eq!(non_addr_lit.verdict, Verdict::Reject, "{:?}", non_addr_lit.diagnostics);
    assert!(non_addr_lit.diagnostics.iter().any(|d| d.code == "HYB-E1002"));

    let map_elem = check(
        "fn main() {\n\
           m := map[string][]int{\"k\": []int{1, 2}}\n\
           _ = m[\"k\"].pop()\n\
         }\n",
    );
    assert_eq!(map_elem.verdict, Verdict::Reject, "{:?}", map_elem.diagnostics);
    assert!(map_elem.diagnostics.iter().any(|d| d.code == "HYB-E1002"));

    let non_slice_arr = check(
        "fn main() {\n\
           var arr [2]int = [1, 2]\n\
           _ = arr.pop()\n\
         }\n",
    );
    assert_eq!(non_slice_arr.verdict, Verdict::Reject, "{:?}", non_slice_arr.diagnostics);
    assert!(non_slice_arr.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
}

#[test]
fn addressable_field_index_paren_slice_pop_and_user_struct_pop_accepted() {
    let r = check(
        "struct holder {\n\
           stack []int\n\
         }\n\
         struct custom_stack {\n\
           top int\n\
         }\n\
         fn (c *custom_stack) pop() int {\n\
           return c.top\n\
         }\n\
         fn main() {\n\
           s := []int{1, 2, 3}\n\
           a := (s).pop()\n\
           h := holder{stack: []int{4, 5}}\n\
           b := h.stack.pop()\n\
           grid := [][]int{[]int{6, 7}}\n\
           c := grid[0].pop()\n\
           cs := custom_stack{top: 8}\n\
           d := cs.pop()\n\
           _ = a + b + c + d\n\
         }\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid addressable slice .pop() and user-defined struct .pop() must be accepted: {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Issue #1822: Non-slice struct, pointer, and interface pop calls rejected
// ---------------------------------------------------------------------------

#[test]
fn struct_and_pointer_without_pop_method_rejected_with_e1002() {
    let struct_val = check(
        "struct point {\n\
           x int\n\
           y int\n\
         }\n\
         fn main() {\n\
           var pt point\n\
           _ = pt.pop()\n\
         }\n",
    );
    assert_eq!(struct_val.verdict, Verdict::Reject, "{:?}", struct_val.diagnostics);
    assert!(
        struct_val.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 on struct without pop method, got: {:?}",
        struct_val.diagnostics
    );

    let struct_ptr = check(
        "struct point {\n\
           x int\n\
           y int\n\
         }\n\
         fn main() {\n\
           var pt point\n\
           var ptr *point = &pt\n\
           _ = ptr.pop()\n\
         }\n",
    );
    assert_eq!(struct_ptr.verdict, Verdict::Reject, "{:?}", struct_ptr.diagnostics);
    assert!(
        struct_ptr.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 on pointer to struct without pop method, got: {:?}",
        struct_ptr.diagnostics
    );
}

#[test]
fn interface_without_pop_method_rejected_with_e1002() {
    let iface = check(
        "fn main() {\n\
           var a any\n\
           _ = a.pop()\n\
         }\n",
    );
    assert_eq!(iface.verdict, Verdict::Reject, "{:?}", iface.diagnostics);
    assert!(
        iface.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 on interface without pop method, got: {:?}",
        iface.diagnostics
    );
}

