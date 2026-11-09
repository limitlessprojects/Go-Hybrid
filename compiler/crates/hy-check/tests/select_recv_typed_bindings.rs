use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f)
}

#[test]
fn select_recv_bindings_enforce_element_const_overflow_in_fn_and_closure() {
    let cases = [
        "fn run(ch chan i8, mch chan map[i8]int) {\n    select {\n        x := <-ch: {\n            x = 300\n            _ = x\n        }\n        m, ok := <-mch: {\n            if ok { m[300] = 1 }\n        }\n        default: {}\n    }\n}\nfn main() {}\n",
        "fn run(ch chan u8) {\n    cb := fn() {\n        select {\n            v := <-ch: {\n                v = 256\n                _ = v\n            }\n            default: {}\n        }\n    }\n    cb()\n}\nfn main() {}\n",
    ];
    for src in cases {
        let res = check(src);
        assert_eq!(res.verdict, Verdict::Reject, "expected HYB-E6002 for:\n{src}");
        assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "diags: {:?}", res.diagnostics);
    }
}

#[test]
fn select_recv_in_range_and_shadowed_outer_var_stay_accepted() {
    let src = "\
fn run(ch chan i8, wch chan int) {
    var x i8 = 1
    _ = x
    select {
        v := <-ch: {
            v = 127
            _ = v
        }
        x, ok := <-wch: {
            if ok {
                x = 300
                _ = x
            }
        }
        default: {}
    }
}
fn main() {}
";
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "unexpected diagnostics: {:?}", res.diagnostics);
}
