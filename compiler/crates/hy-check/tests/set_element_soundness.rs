//! Regression tests for #2501 (`set[T]` element sharing, boundary shapes, and range iteration).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn set_insert_element_freezes_source_and_helper_after_move() {
    let direct = "struct box { m map[int]int }\nfn w(s set[*box], d chan bool) { d <- true }\nfn main() {\n    b := box{m: map[int]int{0: 0}}\n    var s set[*box]\n    _ = s.insert(&b)\n    d := make(chan bool, 1)\n    task w(move s, d)\n    b.m[1] = 2\n}\n";
    assert!(codes(direct).iter().any(|c| c == "HYB-E3003"));

    let helper = "struct counter { n int }\nfn put(s *set[*counter], p *counter) { _ = s.insert(p) }\nfn w(s set[*counter], d chan bool) { d <- true }\nfn main() {\n    c := counter{n: 0}\n    var s set[*counter]\n    put(&s, &c)\n    d := make(chan bool, 1)\n    task w(move s, d)\n    c.n = 2\n}\n";
    assert!(codes(helper).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn set_insert_blocks_non_owned_param_launder_and_borrow_return_escape() {
    let param = "struct box { m map[int]int }\nfn leak(p *box, out chan set[*box]) {\n    var s set[*box]\n    _ = s.insert(p)\n    out <- move s\n}\nfn main() {}\n";
    assert!(codes(param).iter().any(|c| c == "HYB-E3004"));

    let escape = "fn esc() set[*int] {\n    x := 1\n    var s set[*int]\n    _ = s.insert(&x)\n    return s\n}\nfn main() {}\n";
    assert!(codes(escape).iter().any(|c| c == "HYB-E3104"));
}

#[test]
fn set_move_boundary_rejects_foreign_opaque_and_unproven_type_param() {
    let mutex_send = "import \"sync\"\nfn leak(out chan set[*sync.mutex]) {\n    var mu sync.mutex\n    var s set[*sync.mutex]\n    _ = s.insert(&mu)\n    out <- move s\n}\nfn main() {}\n";
    assert!(codes(mutex_send).iter().any(|c| c == "HYB-E3002"));

    let mutex_spawn = "import \"sync\"\nfn w(s set[*sync.mutex]) {}\nfn main() {\n    var mu sync.mutex\n    var s set[*sync.mutex]\n    _ = s.insert(&mu)\n    task w(move s)\n}\n";
    assert!(codes(mutex_spawn).iter().any(|c| c == "HYB-E3001"));

    let generic_set = "fn leak[t](v t, out chan set[t]) {\n    var s set[t]\n    _ = s.insert(v)\n    out <- move s\n}\nfn main() {}\n";
    assert!(codes(generic_set).iter().any(|c| c == "HYB-E3004" || c == "HYB-E3005"));

    let empty_generic_set = "fn leak[t](out chan set[t]) {\n    var s set[t]\n    out <- move s\n}\nfn main() {}\n";
    assert!(codes(empty_generic_set).iter().any(|c| c == "HYB-E3005"));
}

#[test]
fn set_range_resolves_element_shape_for_freeze_and_primitive_send() {
    let ptr_range = "struct box { m map[int]int }\nfn w(s set[*box], d chan bool) { d <- true }\nfn main() {\n    b := box{m: map[int]int{0: 0}}\n    var s set[*box]\n    _ = s.insert(&b)\n    var saved *box = &b\n    for p in s { saved = p }\n    d := make(chan bool, 1)\n    task w(move s, d)\n    saved.m[1] = 2\n}\n";
    assert!(codes(ptr_range).iter().any(|c| c == "HYB-E3003"));

    let ok_prim = "struct box[t] { v t }\nfn relay(n int, out chan set[int]) {\n    var s set[int]\n    _ = s.insert(n)\n    out <- move s\n}\nfn main() {\n    var s set[int]\n    _ = s.insert(7)\n    ch := make(chan int, 1)\n    for x in s { ch <- x }\n    var sb set[box[int]]\n    chb := make(chan set[box[int]], 1)\n    chb <- move sb\n}\n";
    assert!(codes(ok_prim).is_empty(), "expected 0 diagnostics, got {:?}", codes(ok_prim));
}
