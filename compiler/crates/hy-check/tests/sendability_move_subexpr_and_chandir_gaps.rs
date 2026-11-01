use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn move_struct_or_container_with_unproven_atomic_or_foreign_leaf_rejects_e3005() {
    // 1. Struct holding `atomic.value` / `atomic.pointer[int]` moved across task/channel boundary -> HYB-E3005
    let r_atomic_struct = check(
        "import \"sync/atomic\"\n\
         struct atomic_box {\n\
           v atomic.value\n\
           p atomic.pointer[int]\n\
         }\n\
         fn worker(b atomic_box) {\n\
         }\n\
         fn main() {\n\
           var b atomic_box\n\
           task worker(move b)\n\
         }\n",
    );
    assert_eq!(r_atomic_struct.verdict, Verdict::Reject, "{:?}", r_atomic_struct.diagnostics);
    assert!(
        codes(&r_atomic_struct).contains(&"HYB-E3005".to_string()),
        "expected HYB-E3005 for moving struct holding atomic.value / atomic.pointer, got: {:?}",
        r_atomic_struct.diagnostics
    );

    // 2. Struct holding foreign package type `url.values` moved across channel send -> HYB-E3005
    let r_foreign_struct = check(
        "import \"net/url\"\n\
         struct query_box {\n\
           q url.values\n\
         }\n\
         fn main() {\n\
           var qb query_box\n\
           ch := make(chan query_box, 1)\n\
           ch <- move qb\n\
         }\n",
    );
    assert_eq!(r_foreign_struct.verdict, Verdict::Reject, "{:?}", r_foreign_struct.diagnostics);
    assert!(
        codes(&r_foreign_struct).contains(&"HYB-E3005".to_string()),
        "expected HYB-E3005 for moving struct holding url.values, got: {:?}",
        r_foreign_struct.diagnostics
    );

    // 3. Slice / map holding `atomic.value` moved across task boundary -> HYB-E3005
    let r_slice_atomic = check(
        "import \"sync/atomic\"\n\
         fn worker(xs []atomic.value) {\n\
         }\n\
         fn main() {\n\
           var xs []atomic.value\n\
           task worker(move xs)\n\
         }\n",
    );
    assert_eq!(r_slice_atomic.verdict, Verdict::Reject, "{:?}", r_slice_atomic.diagnostics);
    assert!(
        codes(&r_slice_atomic).contains(&"HYB-E3005".to_string()),
        "expected HYB-E3005 for moving slice of atomic.value, got: {:?}",
        r_slice_atomic.diagnostics
    );

    // 4. Positive control: generic struct `box[t]` instantiated with sendable `int` still moves cleanly
    let r_generic_ok = check(
        "import \"fmt\"\n\
         struct box[t] {\n\
           v t\n\
         }\n\
         fn worker(b box[int], done chan<- bool) {\n\
           fmt.println(b.v)\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           b := box[int]{v: 42}\n\
           done := make(chan bool, 1)\n\
           task worker(move b, done)\n\
           <-done\n\
         }\n",
    );
    assert_eq!(r_generic_ok.verdict, Verdict::Accept, "{:?}", r_generic_ok.diagnostics);
}

#[test]
fn move_in_boundary_subexpression_enforces_sendability_and_managed_lint() {
    // 1. `task worker(holder{f: move f, n: 1}.n)` where `f` is a closure -> HYB-E3001
    let r_closure_subexpr = check(
        "struct holder {\n\
           f fn()\n\
           n int\n\
         }\n\
         fn worker(n int) {\n\
         }\n\
         fn main() {\n\
           x := 1\n\
           f := fn() { x = 2 }\n\
           task worker(holder{f: move f, n: 1}.n)\n\
         }\n",
    );
    assert_eq!(
        r_closure_subexpr.verdict,
        Verdict::Reject,
        "{:?}",
        r_closure_subexpr.diagnostics
    );
    assert!(
        codes(&r_closure_subexpr).contains(&"HYB-E3001".to_string()),
        "expected HYB-E3001 when moving closure inside field-projected composite at spawn site, got: {:?}",
        r_closure_subexpr.diagnostics
    );

    // 2. `task worker((move m)["k"])` where `m` has `managed_taint` -> HYB-E8001 + sites_via_managed == 1
    let r_managed_subexpr = check(
        "fn worker(n int) {\n\
         }\n\
         fn main() {\n\
           managed(\"seed map\", \"mutex\") {\n\
             m := map[string]int{\"k\": 7}\n\
             task worker((move m)[\"k\"])\n\
           }\n\
         }\n",
    );
    assert!(
        codes(&r_managed_subexpr).contains(&"HYB-E8001".to_string()),
        "expected HYB-E8001 when moving managed-tainted map in index subexpression at spawn site, got: {:?}",
        r_managed_subexpr.diagnostics
    );
    assert_eq!(r_managed_subexpr.managed.sites_via_managed, 1);

    // 3. `managed(...) { task worker(move []fn(){f}) }` delegates non-ident move classification -> HYB-E8001 + sites_via_managed == 1
    let r_managed_non_ident_move = check(
        "fn worker(fs []fn()) {\n\
         }\n\
         fn main() {\n\
           x := 1\n\
           f := fn() { x = 2 }\n\
           managed(\"actor callbacks\", \"actor\") {\n\
             task worker(move []fn(){f})\n\
           }\n\
         }\n",
    );
    assert!(
        codes(&r_managed_non_ident_move).contains(&"HYB-E8001".to_string()),
        "expected HYB-E8001 for non-ident move of closure slice inside managed block, got: {:?}",
        r_managed_non_ident_move.diagnostics
    );
    assert_eq!(r_managed_non_ident_move.managed.sites_via_managed, 1);
}

#[test]
fn chan_dir_preserved_across_move_multi_return_and_new_allocations() {
    // 1. `move pair_send_only()` in multi-value define, assign, return, and call-arg spread -> HYB-E3008 / HYB-E3009 / HYB-E3012
    let r_move_multi = check(
        "fn pair_send_only() (chan<- int, int) {\n\
           ch := make(chan int, 1)\n\
           return ch, 1\n\
         }\n\
         fn take_recv_only(ch <-chan int, n int) {\n\
         }\n\
         fn bad_return() (<-chan int, int) {\n\
           return move pair_send_only()\n\
         }\n\
         fn main() {\n\
           ch, n := move pair_send_only()\n\
           v := <-ch\n\
           for x in ch {\n\
           }\n\
           var r <-chan int\n\
           var m int\n\
           r, m = move pair_send_only()\n\
           take_recv_only(move pair_send_only())\n\
         }\n",
    );
    assert_eq!(r_move_multi.verdict, Verdict::Reject, "{:?}", r_move_multi.diagnostics);
    let multi_codes = codes(&r_move_multi);
    assert!(
        multi_codes.contains(&"HYB-E3008".to_string()),
        "expected HYB-E3008 for receiving from send-only channel bound via `move pair_send_only()`, got: {:?}",
        r_move_multi.diagnostics
    );
    assert!(
        multi_codes.contains(&"HYB-E3009".to_string()),
        "expected HYB-E3009 for ranging over send-only channel bound via `move pair_send_only()`, got: {:?}",
        r_move_multi.diagnostics
    );
    assert_eq!(
        r_move_multi
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3012")
            .count(),
        3,
        "expected 3 HYB-E3012 diagnostics for return/assign/call-spread of `move pair_send_only()`, got: {:?}",
        r_move_multi.diagnostics
    );

    // 2. `new(T)` pointer-to-array and pointer-to-struct/generic-struct preserve channel direction -> HYB-E3008 / HYB-E3009 / HYB-E3012
    let r_new_alloc = check(
        "type recv_arr = [1]<-chan int\n\
         struct holder {\n\
           ch <-chan int\n\
           out chan<- int\n\
         }\n\
         struct box[t] {\n\
           v t\n\
         }\n\
         fn main() {\n\
           arr_ptr := new(recv_arr)\n\
           arr_ptr[0] <- 1\n\
           close(arr_ptr[0])\n\
           h_ptr := new(holder)\n\
           h_ptr.ch <- 2\n\
           close(h_ptr.ch)\n\
           var s chan<- int = h_ptr.ch\n\
           v := <-h_ptr.out\n\
           b_ptr := new(box[<-chan int])\n\
           b_ptr.v <- 3\n\
         }\n",
    );
    assert_eq!(
        r_new_alloc.verdict,
        Verdict::Reject,
        "{:?}",
        r_new_alloc.diagnostics
    );
    assert_eq!(
        r_new_alloc
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3008")
            .count(),
        4,
        "expected 4 HYB-E3008 diagnostics (arr_ptr[0] <- 1, h_ptr.ch <- 2, <-h_ptr.out, b_ptr.v <- 3), got: {:?}",
        r_new_alloc.diagnostics
    );
    assert_eq!(
        r_new_alloc
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3009")
            .count(),
        2,
        "expected 2 HYB-E3009 diagnostics (close(arr_ptr[0]), close(h_ptr.ch)), got: {:?}",
        r_new_alloc.diagnostics
    );
    assert_eq!(
        r_new_alloc
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3012")
            .count(),
        1,
        "expected 1 HYB-E3012 diagnostic (var s chan<- int = h_ptr.ch), got: {:?}",
        r_new_alloc.diagnostics
    );
}
