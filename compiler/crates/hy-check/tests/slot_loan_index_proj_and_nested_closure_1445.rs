use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn code_lines(src: &str) -> Vec<(String, usize)> {
    let f = parse_file("test.hy", src).expect("parse");
    check_file(&f)
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.span.start.line as usize))
        .collect()
}

fn assert_accept(src: &str) {
    let f = parse_file("test.hy", src).expect("parse");
    let r = check_file(&f);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected 0 diagnostics, got {:?}", r.diagnostics);
}

#[test]
fn reject_addr_of_inline_array_index_projection_on_slot_loan_t24_1445() {
    let src = r#"type trio = [3]int
struct container {
    arr []int
    fixed [3]int
    sl [][2]int
}
fn use_ptr(p *int) { _ = p }
fn main() {
    m2 := map[string]container{"k": container{arr: []int{1}, fixed: [3]int{4, 5, 6}, sl: [][2]int{}}}
    n2 := &m2["k"]
    p2 := &n2.fixed[0]
    _ = p2
    m3 := map[string]container{"k": container{arr: []int{1}, fixed: [3]int{4, 5, 6}, sl: [][2]int{}}}
    n3 := &m3["k"]
    p2_paren := &(n3.fixed)[0]
    _ = p2_paren
    m_arr := map[string]trio{"k": [3]int{1, 2, 3}}
    arr_loan := &m_arr["k"]
    use_ptr(&arr_loan[0])
    f := fn() {
        c := map[string]container{"k": container{arr: []int{1}, fixed: [3]int{1, 2, 3}, sl: [][2]int{}}}
        s := &c["k"]
        q := &s.fixed[0]
        _ = q
    }
    f()
    f_captured := fn() {
        q2 := &n2.fixed[0]
        _ = q2
    }
    f_captured()
}
"#;
    assert_eq!(
        code_lines(src),
        vec![
            ("HYB-E3104".to_string(), 11),
            ("HYB-E3104".to_string(), 15),
            ("HYB-E3104".to_string(), 19),
            ("HYB-E3104".to_string(), 23),
            ("HYB-E3104".to_string(), 28),
        ]
    );

    let src_no_t24_on_slice_or_ptr = r#"struct inner_val { x int }
struct container {
    arr []int
    fixed [3]int
    sl [][2]int
}
fn check_ptr_field(m_ptr map[string]*inner_val) {
    n := &m_ptr["k"]
    p_ptr := &n.x
    _ = p_ptr
}
fn main() {
    m1 := map[string]container{"k": container{arr: []int{1, 2, 3}, fixed: [3]int{4, 5, 6}, sl: [][2]int{[2]int{8, 9}}}}
    n1 := &m1["k"]
    p_slice := &n1.arr[0]
    _ = p_slice
    n2 := &m1["k"]
    p_slice_of_arr := &n2.sl[0][1]
    _ = p_slice_of_arr
}
"#;
    assert!(
        code_lines(src_no_t24_on_slice_or_ptr).iter().all(|(c, _)| c != "HYB-E3104"),
        "slice-backed and pointer-field projections must not emit T24 HYB-E3104"
    );

    let src_accept = r#"struct container {
    arr []int
    fixed [3]int
    sl [][2]int
}
fn main() {
    local1 := container{arr: []int{1, 2}, fixed: [3]int{3, 4, 5}, sl: [][2]int{[2]int{8, 9}}}
    p_local := &local1.fixed[0]
    _ = p_local
    local2 := container{arr: []int{1, 2}, fixed: [3]int{3, 4, 5}, sl: [][2]int{[2]int{8, 9}}}
    p_slice := &local2.arr[0]
    _ = p_slice
    local3 := container{arr: []int{1, 2}, fixed: [3]int{3, 4, 5}, sl: [][2]int{[2]int{8, 9}}}
    p_slice_of_arr := &local3.sl[0][1]
    _ = p_slice_of_arr
    local4 := container{arr: []int{1, 2}, fixed: [3]int{3, 4, 5}, sl: [][2]int{[2]int{8, 9}}}
    p_local_borrow := &local4
    f_non_loan_capture := fn() {
        q_ok := &p_local_borrow.fixed[0]
        _ = q_ok
    }
    f_non_loan_capture()
}
"#;
    assert_accept(src_accept);
}

#[test]
fn reject_nested_closure_captured_slot_loan_escape_and_accept_shadowed_or_value_return_1445() {
    let src = r#"struct node {
    val int
}
fn main() {
    outer := fn() *node {
        m2 := map[string]node{"a": node{val: 1}}
        s2 := &m2["a"]
        inner := fn() *node {
            return s2
        }
        return inner()
    }
    outer_late_rebind := fn(p *node, c bool) *node {
        m2 := map[string]node{"a": node{val: 1}}
        s3 := p
        inner2 := fn() *node {
            return s3
        }
        s3 = &m2["a"]
        if c {
            var s3 node
            _ = s3
        }
        inner_after_shadow := fn() *node {
            return s3
        }
        _ = inner_after_shadow
        return inner2()
    }
    outer_local_borrow_after_shadow := fn(c bool) *node {
        v := node{val: 1}
        p := &v
        if c {
            p := 1
            _ = p
        }
        return p
    }
    _ = outer
    _ = outer_late_rebind
    _ = outer_local_borrow_after_shadow
}
"#;
    assert_eq!(
        code_lines(src),
        vec![
            ("HYB-E3104".to_string(), 9),
            ("HYB-E3104".to_string(), 17),
            ("HYB-E3104".to_string(), 25),
            ("HYB-E3104".to_string(), 37),
        ]
    );

    let src_accept = r#"struct node {
    val int
}
fn main() {
    v_local := node{val: 1}
    p_local := &v_local
    get_local := fn() *node { return p_local }
    _ = get_local()
    outer := fn(ext *node) int {
        m2 := map[string]node{"a": node{val: 1}}
        s2 := &m2["a"]
        p := ext
        if true {
            p := &m2["a"]
            _ = p
            s2 := ext
            shadow_block := fn() *node { return s2 }
            _ = shadow_block()
        }
        after_inner_loan := fn() *node { return p }
        read_val := fn() int { return s2.val }
        pass_ext := fn() *node { return ext }
        shadow_param := fn(s2 *node) *node { return s2 }
        shadow_ptr := fn() *node {
            s2 := ext
            return s2
        }
        shadow_local := fn() node {
            s2 := node{val: 9}
            return s2
        }
        shadow_var := fn() node {
            var s2 node
            return s2
        }
        shadow_comma_ok := fn() node {
            s2, ok := m2["a"]
            _ = ok
            return s2
        }
        _ = after_inner_loan()
        _ = pass_ext()
        _ = shadow_param(ext)
        _ = shadow_ptr()
        _ = shadow_local()
        _ = shadow_var()
        _ = shadow_comma_ok()
        return read_val()
    }
    _ = outer
}
"#;
    assert_accept(src_accept);
}
