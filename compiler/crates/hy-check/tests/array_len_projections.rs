use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f)
}

#[test]
fn rejects_oob_on_file_const_struct_field_multidim_call_receiver_and_named_result() {
    let cases = [
        "const arr = [3]int{1, 2, 3}\nfn main() { _ = arr[3] }\n",
        "const arr = [...]int{1, 2}\nfn main() { _ = arr[0:3] }\n",
        "struct holder { arr [3]int }\nfn main() { var h holder\nh.arr[3] = 1 }\n",
        "fn main() { var grid [2][3]int\ngrid[0][3] = 2 }\n",
        "fn make_arr() [3]int { return [3]int{1, 2, 3} }\nfn main() { _ = make_arr()[3] }\n",
        "struct box { arr [3]int }\nfn (b box) bad() int { return b.arr[3] }\nfn main() {}\n",
        "type arr2 = [2]int\nfn bad() (out arr2) { out[2] = 1\nreturn }\nfn main() {}\n",
    ];
    for src in cases {
        let res = check(src);
        assert_eq!(res.verdict, Verdict::Reject, "expected HYB-E1002 for:\n{src}");
        assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E1002"), "diags: {:?}", res.diagnostics);
    }
}

#[test]
fn accepts_in_bounds_projections_and_shadowing_closure_param() {
    let src = "\
const arr = [3]int{1, 2, 3}
struct holder { arr [4]int }
fn make_arr() [3]int { return arr }
fn main() {
    var h holder
    var grid [2][3]int
    h.arr[3] = arr[2] + make_arr()[2]
    grid[1][2] = h.arr[3]
    wider := fn(arr [6]int) int { return arr[5] }
    _ = wider([6]int{1, 2, 3, 4, 5, 6})
}
";
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "unexpected diagnostics: {:?}", res.diagnostics);
}
