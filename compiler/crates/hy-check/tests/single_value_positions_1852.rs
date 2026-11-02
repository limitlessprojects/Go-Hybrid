use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn single_value_positions_reject_void_and_multi_value_calls_e1002() {
    for body in [
        "return 1, pair()",
        "return 1, noop()",
        "_ = slices()[0]\n    return 1, 2",
        "_ = slices()[0:1]\n    return 1, 2",
        "_ = <-chans()\n    return 1, 2",
        "if bools() {}\n    return 1, 2",
        "for x in slices() { _ = x }\n    return 1, 2",
        "a, b := 1, noop()\n    return a, b",
        "mut a := 1\n    mut b := 2\n    a, b = 1, noop()\n    return a, b",
    ] {
        let src = format!(
            "fn noop() {{}}\nfn pair() (int, int) {{ return 1, 2 }}\nfn bools() (bool, bool) {{ return true, false }}\nfn slices() ([]int, []int) {{ return []int{{1}}, []int{{2}} }}\nfn chans() (chan int, chan int) {{ return make(chan int), make(chan int) }}\nfn f() (int, int) {{\n    {body}\n}}\n"
        );
        let d = check_file(&parse_file("t.hy", &src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{body}: {d:?}");
    }
}
