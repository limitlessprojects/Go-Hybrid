use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

#[test]
fn negative_regression_direct_slice_move_freezes_referenced_local() {
    let src = r#"
struct node {
    val int
}

fn worker(s []*node, done chan int) {
    s[0].val = 999
    done <- 1
}

fn main() {
    n := node{val: 0}
    s := []*node{&n}
    done := make(chan int, 1)
    task worker(move s, done)
    n.val = 1
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn negative_regression_param_pointer_slice_move_is_e3004() {
    let src = r#"
struct node {
    val int
}

fn worker(s []*node, done chan int) {
    s[0].val = 999
    done <- 1
}

fn run(n node, done chan int) {
    s := []*node{&n}
    task worker(move s, done)
}

fn main() {
    n := node{val: 0}
    done := make(chan int, 1)
    run(n, done)
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
}

#[test]
fn negative_regression_const_mutation_through_pointer_slice_is_e3006() {
    let src = r#"
struct node {
    val int
}

const global_node = node{val: 0}

fn main() {
    s := []*node{&global_node}
    s[0].val = 999
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3006"));
}

#[test]
fn negative_regression_struct_field_holding_pointer_slice_is_e3103() {
    let src = r#"
struct node {
    val int
}

struct box_holder {
    nodes []*node
}

fn main() {
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3103"));
}

#[test]
fn negative_regression_struct_field_holding_borrow_handle_slice_is_e3103() {
    let src = r#"
struct box_holder {
    views []serde_json.view
}

fn main() {
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3103"));
}

#[test]
fn positive_control_owned_move_without_post_use_accepts() {
    let src = r#"
struct node {
    val int
}

fn worker(s []*node, done chan int) {
    s[0].val = 999
    done <- 1
}

fn main() {
    n := node{val: 0}
    s := []*node{&n}
    done := make(chan int, 1)
    task worker(move s, done)
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn positive_control_scalar_atomic_pointer_field_accepts() {
    let src = r#"
struct box_holder {
    p *atomic.int64
}

fn main() {
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn edge_case_map_of_pointers() {
    let src = r#"
struct node {
    val int
}

fn worker(m map[string]*node, done chan int) {
    m["k"].val = 999
    done <- 1
}

fn main() {
    n := node{val: 0}
    m := map[string]*node{"k": &n}
    done := make(chan int, 1)
    task worker(move m, done)
    n.val = 1
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn edge_case_array_of_pointers() {
    let src = r#"
struct node {
    val int
}

fn worker(a []*node, done chan int) {
    a[0].val = 999
    done <- 1
}

fn main() {
    n := node{val: 0}
    a := [&n]
    done := make(chan int, 1)
    task worker(move a, done)
    n.val = 1
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn edge_case_chan_send_move_freezes_referenced_local() {
    let src = r#"
struct node {
    val int
}

fn worker(ch chan []*node, done chan int) {
    s := <-ch
    s[0].val = 999
    done <- 1
}

fn main() {
    n := node{val: 0}
    s := []*node{&n}
    ch := make(chan []*node, 1)
    done := make(chan int, 1)
    task worker(ch, done)
    ch <- move s
    n.val = 1
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn multiple_borrow_roots_in_slice_freezes_first_root() {
    let src = r#"
struct node {
    val int
}

fn worker(s []*node, done chan int) {
    s[0].val = 999
    done <- 1
}

fn main() {
    n1 := node{val: 0}
    n2 := node{val: 0}
    s := []*node{&n1, &n2}
    done := make(chan int, 1)
    task worker(move s, done)
    n1.val = 1
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn multiple_borrow_roots_in_slice_freezes_second_root() {
    let src = r#"
struct node {
    val int
}

fn worker(s []*node, done chan int) {
    s[1].val = 999
    done <- 1
}

fn main() {
    n1 := node{val: 0}
    n2 := node{val: 0}
    s := []*node{&n1, &n2}
    done := make(chan int, 1)
    task worker(move s, done)
    n2.val = 2
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn deeply_nested_slice_of_slices_freezes_all_roots() {
    let src = r#"
struct node {
    val int
}

fn worker_grid(grid [][]*node, done chan int) {
    grid[0][0].val = 999
    done <- 1
}

fn main() {
    n1 := node{val: 0}
    n2 := node{val: 0}
    grid := [][]*node{ []*node{&n1}, []*node{&n2} }
    done := make(chan int, 1)
    task worker_grid(move grid, done)
    n1.val = 1
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn deeply_nested_slice_of_slices_freezes_second_root() {
    let src = r#"
struct node {
    val int
}

fn worker_grid(grid [][]*node, done chan int) {
    grid[1][0].val = 999
    done <- 1
}

fn main() {
    n1 := node{val: 0}
    n2 := node{val: 0}
    grid := [][]*node{ []*node{&n1}, []*node{&n2} }
    done := make(chan int, 1)
    task worker_grid(move grid, done)
    n2.val = 2
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn deeply_nested_map_of_pointer_slices_freezes_root() {
    let src = r#"
struct node {
    val int
}

fn worker_map(m map[string][]*node, done chan int) {
    m["row"][0].val = 999
    done <- 1
}

fn main() {
    n := node{val: 0}
    m := map[string][]*node{"row": []*node{&n}}
    done := make(chan int, 1)
    task worker_map(move m, done)
    n.val = 1
    <-done
}
"#;
    let res = check(src);
    assert!(matches!(res.verdict, Verdict::Reject));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn positive_control_multiple_borrow_roots_unaccessed_post_move() {
    let src = r#"
struct node {
    val int
}

fn worker(s []*node, done chan int) {
    s[0].val = 999
    done <- 1
}

fn main() {
    n1 := node{val: 0}
    n2 := node{val: 0}
    s := []*node{&n1, &n2}
    done := make(chan int, 1)
    task worker(move s, done)
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn positive_control_deeply_nested_composites_unaccessed_post_move() {
    let src = r#"
struct node {
    val int
}

fn worker_grid(grid [][]*node, done chan int) {
    grid[0][0].val = 999
    done <- 1
}

fn main() {
    n1 := node{val: 0}
    n2 := node{val: 0}
    grid := [][]*node{ []*node{&n1}, []*node{&n2} }
    done := make(chan int, 1)
    task worker_grid(move grid, done)
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_call_returning_int_accepts() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn count_items(n *node) int {
    return n.val
}

fn worker(n node) {}

fn main() {
    n := node{val: 42}
    count := count_items(&n)
    task worker(move n)
    fmt.println(count)
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_call_returning_bool_accepts() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn is_valid(n *node) bool {
    return n.val > 0
}

fn worker(n node) {}

fn main() {
    n := node{val: 42}
    ok := is_valid(&n)
    task worker(move n)
    fmt.println(ok)
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_len_of_call_accepts() {
    let src = r#"
import "fmt"

struct node {
    tag string
}

fn get_tag(n *node) string {
    return n.tag
}

fn worker(n node) {}

fn main() {
    n := node{tag: "hello"}
    l := len(get_tag(&n))
    task worker(move n)
    fmt.println(l)
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_comparison_involving_call_accepts() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn get_val(n *node) int {
    return n.val
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    is_positive := get_val(&n) > 0
    task worker(move n)
    fmt.println(is_positive)
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_slice_of_scalar_call_results_accepts() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn get_val(n *node) int {
    return n.val
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    items := []int{get_val(&n)}
    task worker(move n)
    fmt.println(items[0])
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_map_of_scalar_call_results_accepts() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn get_val(n *node) int {
    return n.val
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    table := map[string]int{"x": get_val(&n)}
    task worker(move n)
    fmt.println(table["x"])
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_method_call_returning_scalar_accepts() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn (n node) get_with_ref(other *node) int {
    return n.val + other.val
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    other := node{val: 20}
    m_val := other.get_with_ref(&n)
    task worker(move n)
    fmt.println(m_val)
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn negative_control_cast_conversion_returning_scalar_accepts() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn get_val(n *node) int {
    return n.val
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    cast_val := int(get_val(&n))
    task worker(move n)
    fmt.println(cast_val)
}
"#;
    let res = check(src);
    assert_eq!(res.diagnostics.len(), 0, "{:?}", res.diagnostics);
    assert!(matches!(res.verdict, Verdict::Accept));
}

#[test]
fn reslice_of_pointer_slice_literal_freezes_borrowed_value_struct() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    ps := []*node{&n}[:]
    task worker(move n)
    fmt.println(ps[0].val)
}
"#;
    let res = check(src);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for resliced pointer slice literal after move n, got {:?}",
        res.diagnostics
    );
}

#[test]
fn index_into_pointer_slice_literal_freezes_borrowed_value_struct() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    p := []*node{&n}[0]
    task worker(move n)
    fmt.println(p.val)
}
"#;
    let res = check(src);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for indexed pointer slice literal after move n, got {:?}",
        res.diagnostics
    );
}

#[test]
fn append_with_addr_of_value_struct_freezes_borrowed_value_struct() {
    let src = r#"
import "fmt"

struct node {
    val int
}

fn worker(n node) {}

fn main() {
    n := node{val: 10}
    ps := append([]*node{}, &n)
    task worker(move n)
    fmt.println(ps[0].val)
}
"#;
    let res = check(src);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for append([]*node{{}}, &n) after move n, got {:?}",
        res.diagnostics
    );
}

#[test]
fn returning_composite_or_map_containing_bound_borrow_rejects_loan_escape() {
    let src1 = r#"
struct node {
    val int
}

fn bad_slice() []*node {
    n := node{val: 1}
    p := &n
    return []*node{p}
}

fn main() {}
"#;
    let res1 = check(src1);
    assert!(
        res1.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 when returning []*node{{p}} containing local borrow p, got {:?}",
        res1.diagnostics
    );

    let src2 = r#"
struct node {
    val int
}

fn bad_map() map[string]*node {
    n := node{val: 1}
    mp := map[string]*node{}
    mp["a"] = &n
    return mp
}

fn main() {}
"#;
    let res2 = check(src2);
    assert!(
        res2.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 when returning map tainted by mp[\"a\"] = &n, got {:?}",
        res2.diagnostics
    );
}

#[test]
fn block_self_shadowing_with_addr_of_freezes_outer_binding_after_scope_pop() {
    let src = r#"
import "fmt"

struct holder {
    m map[string]int
}

fn worker(h holder) {}

fn main() {
    h := holder{m: map[string]int{"a": 1}}
    other := holder{m: map[string]int{"b": 2}}
    p := &other
    if true {
        h := &h
        p = h
    }
    task worker(move h)
    fmt.println(p.m["a"])
}
"#;
    let res = check(src);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when p receives &h via self-shadowing h := &h and outer h is moved, got {:?}",
        res.diagnostics
    );
}

#[test]
fn multi_value_and_closure_array_slice_share_freezes_backing_array() {
    let src1 = r#"
import "fmt"

fn worker(s []int) {}

fn main() {
    var a [3]int
    m := map[string][]int{"k": a[:]}
    s, ok := m["k"]
    task worker(move s)
    if ok {
        fmt.println(a[0])
    }
}
"#;
    let res1 = check(src1);
    assert!(
        res1.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when multi-value map lookup unpacks slice backed by local array a and moves it, got {:?}",
        res1.diagnostics
    );

    let src2 = r#"
import "fmt"

fn worker(s []int) {}

fn main() {
    var a [3]int
    f := fn() int {
        return a[0]
    }
    g := f
    s := a[:]
    task worker(move s)
    fmt.println(g())
}
"#;
    let res2 = check(src2);
    assert!(
        res2.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when slice of local array a moves and closure copy g reading a is called, got {:?}",
        res2.diagnostics
    );
}

#[test]
fn multi_value_field_store_links_may_share_and_freezes_source() {
    let src = r#"
import "fmt"

struct holder {
    m map[string]int
}

fn worker(h holder) {}

fn main() {
    buckets := map[string]map[string]int{"k": map[string]int{"x": 1}}
    h := holder{m: map[string]int{}}
    ok := false
    h.m, ok = buckets["k"]
    task worker(move h)
    if ok {
        fmt.println(buckets["k"]["x"])
    }
}
"#;
    let res = check(src);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when multi-value unpack into h.m moves h and source map is read, got {:?}",
        res.diagnostics
    );
}

#[test]
fn closure_and_interproc_field_store_and_multi_value_const_write_rejects_e3006() {
    let src1 = r#"
const const_buckets = map[string]map[string]int{"k": map[string]int{"x": 1}}

fn main() {
    f := fn() {
        m, ok := const_buckets["k"]
        if ok {
            m["x"] = 2
        }
    }
    f()
}
"#;
    let res1 = check(src1);
    assert!(
        res1.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when closure unpacks const map via multi-value lookup and mutates it, got {:?}",
        res1.diagnostics
    );

    let src2 = r#"
const const_map = map[string]int{"a": 1}

struct holder {
    m map[string]int
}

fn mutate_via_field(src map[string]int) {
    h := holder{m: map[string]int{}}
    h.m = src
    h.m["a"] = 99
}

fn main() {
    mutate_via_field(const_map)
}
"#;
    let res2 = check(src2);
    assert!(
        res2.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when callee stores const map parameter into struct field and mutates it, got {:?}",
        res2.diagnostics
    );
}
