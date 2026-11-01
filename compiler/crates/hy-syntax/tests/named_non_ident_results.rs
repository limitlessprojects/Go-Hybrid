use hy_syntax::{parse_file, ChanDir, TypeExpr};

#[test]
fn parses_named_slice_array_pointer_chan_and_fn_results() {
    let src = "\
struct node { v int }
fn items() (out []int, err error) { return }
fn ptr_arr(n *node) (out *node, arr [2]int, recv <-chan int) { return }
fn ch_fn() (send chan<- int, both chan int, cb fn(int) int) { return }
";
    let file = parse_file("test.hy", src).expect("named non-ident result lists should parse");
    let fns: Vec<_> = file.fns().collect();
    assert_eq!(fns[0].results[0].name.as_deref(), Some("out"));
    assert!(matches!(fns[0].results[0].ty, TypeExpr::Slice { .. }));
    assert_eq!(fns[0].results[1].name.as_deref(), Some("err"));
    assert_eq!(fns[1].results[0].name.as_deref(), Some("out"));
    assert!(matches!(fns[1].results[0].ty, TypeExpr::Pointer { .. }));
    assert_eq!(fns[1].results[1].name.as_deref(), Some("arr"));
    assert!(matches!(fns[1].results[1].ty, TypeExpr::Array { .. }));
    assert_eq!(fns[1].results[2].name.as_deref(), Some("recv"));
    assert!(matches!(fns[1].results[2].ty, TypeExpr::Chan { dir: ChanDir::Recv, .. }));
    assert_eq!(fns[2].results[0].name.as_deref(), Some("send"));
    assert!(matches!(fns[2].results[0].ty, TypeExpr::Chan { dir: ChanDir::Send, .. }));
    assert_eq!(fns[2].results[1].name.as_deref(), Some("both"));
    assert_eq!(fns[2].results[2].name.as_deref(), Some("cb"));
    assert!(matches!(fns[2].results[2].ty, TypeExpr::Fn { .. }));
}

#[test]
fn preserves_unnamed_generic_and_map_result_lists() {
    let src = "\
struct box[t] { v t }
fn unnamed() (option[int], map[string][]int, box[map[string]int], error) {
    return none, map[string][]int{}, box[map[string]int]{v: map[string]int{}}, nil
}
";
    let file = parse_file("test.hy", src).expect("unnamed generic and map result lists still parse");
    let f = file.fns().next().unwrap();
    assert!(f.results.iter().all(|r| r.name.is_none()));
    assert!(matches!(f.results[0].ty, TypeExpr::Option { .. }));
    assert!(matches!(f.results[1].ty, TypeExpr::Map { .. }));
}
