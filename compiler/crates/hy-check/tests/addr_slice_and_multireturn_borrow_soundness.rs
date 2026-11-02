//! Address-of must retain pointer type and backing-storage provenance.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn verify(src: &str, expected: Option<&str>) {
    let ast = parse_file("addr_slice_borrow.hy", src).expect("fixture must parse");
    let result = check_file(&ast);
    if let Some(code) = expected {
        assert_eq!(result.verdict, Verdict::Reject, "{:#?}", result.diagnostics);
        assert!(
            result.diagnostics.iter().any(|d| d.code == code),
            "expected {code}, got {:#?}",
            result.diagnostics
        );
    } else {
        assert_eq!(result.verdict, Verdict::Accept, "{:#?}", result.diagnostics);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }
}

#[test]
fn subslice_task_reject() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 _ = xs
 task worker(&xs[0:][0]) }
"#,
        Some("HYB-E3001"),
    );
}

#[test]
fn subslice_send_reject() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 _ = xs
 ch := make(chan *int, 1)
 ch <- &xs[0:][0] }
"#,
        Some("HYB-E3002"),
    );
}

#[test]
fn call_task_reject() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 _ = xs
 task worker(&id(xs)[0]) }
"#,
        Some("HYB-E3001"),
    );
}

#[test]
fn call_send_reject() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 _ = xs
 ch := make(chan *int, 1)
 ch <- &id(xs)[0] }
"#,
        Some("HYB-E3002"),
    );
}

#[test]
fn literal_task_reject() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 _ = xs
 task worker(&([]int{1, 2}[0])) }
"#,
        Some("HYB-E3001"),
    );
}

#[test]
fn literal_send_reject() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 _ = xs
 ch := make(chan *int, 1)
 ch <- &([]int{1, 2}[0]) }
"#,
        Some("HYB-E3002"),
    );
}

#[test]
fn subslice_move_freezes_root() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 p := &xs[0:][0]
 task worker(move p)
 xs[0] = 3 }
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn subslice_param_origin() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn relay(xs []int) { p := &xs[0:][0]
 task worker(move p) }
"#,
        Some("HYB-E3004"),
    );
}

#[test]
fn subslice_local_control() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 p := &xs[0:][0]
 _ = p
 xs[0] = 3 }
"#,
        None,
    );
}

#[test]
fn call_move_freezes_root() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 p := &id(xs)[0]
 task worker(move p)
 xs[0] = 3 }
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn call_param_origin() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn relay(xs []int) { p := &id(xs)[0]
 task worker(move p) }
"#,
        Some("HYB-E3004"),
    );
}

#[test]
fn call_local_control() {
    verify(
        r#"
fn id(xs []int) []int { return xs }
fn worker(p *int) {}
fn main() { xs := []int{1, 2}
 p := &id(xs)[0]
 _ = p
 xs[0] = 3 }
"#,
        None,
    );
}

#[test]
fn const_paren() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { p := &(xs[0])
 _ = p }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_arg_paren() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { write_ptr(&(xs[0])) }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_subslice() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { p := &xs[0:][0]
 _ = p }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_arg_subslice() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { write_ptr(&xs[0:][0]) }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_call() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { p := &id(xs)[0]
 _ = p }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_arg_call() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { write_ptr(&id(xs)[0]) }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_paren_call() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { p := &(id(xs)[0])
 _ = p }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_arg_paren_call() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn main() { write_ptr(&(id(xs)[0])) }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_paren_field() {
    verify(
        r#"
struct box { val int }
const b = box{val: 1}
fn main() { p := &(b.val)
 _ = p }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn callee_const_return() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn get() []cell { return xs }
fn main() { p := &get()[0]
 _ = p }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn callee_const_return_arg() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn get() []cell { return xs }
fn main() { write_ptr(&get()[0]) }
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn multi_return_escape() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn bad() *stats { st := stats{val: 0}
 p, k := borrow_pair(&st)
 _ = k
 return p }
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn multi_return_freeze() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn main() { st := stats{val: 0}
 p, k := borrow_pair(&st)
 task worker(move p)
 _ = k
 st.val = 1 }
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn multi_return_param_origin() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn relay(st stats) { p, k := borrow_pair(&st)
 _ = k
 task worker(move p) }
"#,
        Some("HYB-E3004"),
    );
}

#[test]
fn multi_return_scalar_control() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn good() int { st := stats{val: 0}
 _, k := borrow_pair(&st)
 return k }
"#,
        None,
    );
}

#[test]
fn multi_return_local_control() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn main() { st := stats{val: 0}
 p, k := borrow_pair(&st)
 p.val = k }
"#,
        None,
    );
}

#[test]
fn rebind_call_escape() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn bad() *stats { st := stats{val: 0}
 var p *stats
 p = id_ptr(&st)
 return p }
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn rebind_call_control() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn main() { st := stats{val: 0}
 var p *stats
 p = id_ptr(&st)
 p.val = 2 }
"#,
        None,
    );
}

#[test]
fn rebind_projection_escape() {
    // O12 also rejects the pointer field (E3103); the return must independently
    // retain borrow provenance and report E3104.
    verify(
        r#"
struct stats { val int }
struct ptr_box { p *stats }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn bad() *stats { st := stats{val: 0}
 var p *stats
 p = ptr_box{p: &st}.p
 return p }
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn rebind_projection_control() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn id_ptr(p *stats) *stats { return p }
fn worker(p *stats) {}
fn main() { st := stats{val: 0}
 var p *stats
 p = []*stats{&st}[0]
 p.val = 2 }
"#,
        None,
    );
}

#[test]
fn array_call_copy_control() {
    verify(
        r#"
fn clone_arr(a [2]int) [2]int { return a }
fn worker(xs []int) {}
fn main() { var a [2]int
 c := clone_arr(a)
 xs := c[:]
 task worker(move xs)
 a[0] = 1 }
"#,
        None,
    );
}

#[test]
fn fresh_slice_const_control() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn clone(xs []cell) []cell { return append([]cell{}, xs...) }
fn main() { p := &clone(xs)[0]
 _ = p }
"#,
        None,
    );
}

#[test]
fn fresh_slice_const_arg_control() {
    verify(
        r#"
struct cell { val int }
const xs = []cell{cell{val: 1}, cell{val: 2}}
fn id(xs []cell) []cell { return xs }
fn write_ptr(p *cell) { p.val = 3 }
fn clone(xs []cell) []cell { return append([]cell{}, xs...) }
fn main() { write_ptr(&clone(xs)[0]) }
"#,
        None,
    );
}

#[test]
fn channel_pointer_control() {
    verify(
        r#"
fn id(xs []chan int) []chan int { return xs }
fn worker(p *chan int) {}
fn main() { xs := []chan int{make(chan int)}
 task worker(&id(xs)[0]) }
"#,
        None,
    );
}

#[test]
fn multi_return_pointer_in_second_slot_escapes() {
    verify(
        r#"
struct stats { val int }
fn pair(p *stats) (int, *stats) { return 1, p }
fn bad() *stats {
    st := stats{val: 0}
    k, p := pair(&st)
    _ = k
    return p
}
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn multi_return_borrow_links_all_roots() {
    verify(
        r#"
struct stats { val int }
fn pair(a *stats, b *stats) (*stats, int) {
    if true { return a, 1 }
    return b, 2
}
fn worker(p *stats) {}
fn main() {
    a := stats{val: 0}
    b := stats{val: 0}
    p, _ := pair(&a, &b)
    task worker(move p)
    a.val = 1
    b.val = 2
}
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn unknown_unrooted_pointee_fails_closed() {
    verify(
        r#"
fn worker(p *int) {}
fn main() { task worker(&missing()[0]) }
"#,
        Some("HYB-E3005"),
    );
}

#[test]
fn value_returning_call_rebind_is_not_borrow() {
    verify(
        r#"
struct stats { val int }
fn read(p *stats) int { return p.val }
fn good() int {
    st := stats{val: 2}
    n := 0
    n = read(&st)
    return n
}
"#,
        None,
    );
}

#[test]
fn array_copy_address_does_not_freeze_original() {
    verify(
        r#"
fn clone_arr(a [2]int) [2]int { return a }
fn worker(p *int) {}
fn main() {
    var a [2]int
    p := &clone_arr(a)[:][0]
    task worker(move p)
    a[0] = 1
}
"#,
        // Go rejects slicing the unaddressable call result (#2789).
        Some("HYB-E1002"),
    );
}

#[test]
fn multi_return_assign_escape() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn bad() *stats { st := stats{val: 0}
 var p *stats
 var k int
 p, k = borrow_pair(&st)
 _ = k
 return p }
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn multi_return_assign_freeze() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn worker(p *stats) {}
fn main() { st := stats{val: 0}
 var p *stats
 var k int
 p, k = borrow_pair(&st)
 task worker(move p)
 _ = k
 st.val = 1 }
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn multi_return_assign_param_origin() {
    verify(
        r#"
struct stats { val int }
fn borrow_pair(p *stats) (*stats, int) { return p, 1 }
fn worker(p *stats) {}
fn relay(st stats) {
 var p *stats
 var k int
 p, k = borrow_pair(&st)
 _ = k
 task worker(move p) }
"#,
        Some("HYB-E3004"),
    );
}

#[test]
fn rebind_borrow_links_all_roots() {
    verify(
        r#"
struct stats { val int }
fn pick(a *stats, b *stats) *stats {
 if true { return a }
 return b
}
fn worker(p *stats) {}
fn main() {
 a := stats{val: 0}
 b := stats{val: 0}
 var p *stats
 p = pick(&a, &b)
 task worker(move p)
 b.val = 2 }
"#,
        Some("HYB-E3003"),
    );
}
