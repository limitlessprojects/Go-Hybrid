use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("inline.hy", src).expect("must parse"))
}

#[test]
fn channel_element_composite_and_primitive_mismatches_reject_e3011() {
    let reject_cases = [
        "fn main() {
    ch := make(chan int, 1)
    ch <- true
}
",
        "fn main() {
    ch := make(chan bool, 1)
    ch <- 42
}
",
        "fn main() {
    ch := make(chan int, 1)
    var f float64 = 1.5
    ch <- f
}
",
        "fn main() {
    ch := make(chan []int, 1)
    xs := []string{"a"}
    ch <- move xs
}
",
        "fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]string{"a": "b"}
    ch <- move m
}
",
        "fn main() {
    ch := make(chan [2]int, 1)
    xs := []int{1, 2}
    ch <- move xs
}
",
        "fn main() {
    ch := make(chan [2]int, 1)
    ch <- [2]int{1, 2}
    var xs []int = <-ch
}
",
    ];
    for src in reject_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == "HYB-E3011"));
    }

    let ok = check("fn main() {
    ch_b := make(chan bool, 1)
    ch_b <- true
    b := <-ch_b
    ch_s := make(chan []int, 1)
    xs := []int{1, 2}
    ch_s <- move xs
    var got_s []int = <-ch_s
    fmt.println("{b} {len(got_s)}")
}
");
    assert_eq!(ok.verdict, Verdict::Accept);
}
