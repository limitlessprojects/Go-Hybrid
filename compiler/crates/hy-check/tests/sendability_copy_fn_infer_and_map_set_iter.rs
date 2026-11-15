use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("<test>", src).expect("parse ok");
    check_file(&f)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn copy_slice_chan_dir_mismatch_rejected() {
    assert!(codes("fn f(d []chan<- int, s []<-chan int) { copy(d, s) }").iter().any(|c| c == "HYB-E3012"));
    assert!(codes("fn f(d []chan<- int, s []chan int) { copy(d, s) }").iter().any(|c| c == "HYB-E3012"));
    assert!(codes("fn f(d []chan<- int, s []chan<- int) { copy(d, s) }").is_empty());
}

#[test]
fn generic_fn_param_chan_dir_inference() {
    let supply = "fn sup[t](mk fn() t) t { return mk() }\nfn f(rx <-chan int) {\nvar d chan<- int = sup(fn() <-chan int { return rx })\n_ = d\n}";
    assert!(codes(supply).iter().any(|c| c == "HYB-E3012"));
    let pair = "fn both[t](a fn(c t), b fn(c t)) {\n_ = a\n_ = b\n}\nfn f() { both(fn(c chan<- int) {}, fn(c <-chan int) {}) }";
    assert!(codes(pair).iter().any(|c| c == "HYB-E3012"));
    let ok = "fn both[t](a fn(c t), b fn(c t)) {\n_ = a\n_ = b\n}\nfn f() { both(fn(c chan<- int) {}, fn(c chan<- int) {}) }";
    assert!(codes(ok).is_empty());
}

#[test]
fn single_var_map_and_set_iteration_chan_dirs() {
    assert!(codes("fn f(m map[<-chan int]chan int) { for k in m { close(k) } }").iter().any(|c| c == "HYB-E3009"));
    assert!(codes("fn f(m map[<-chan int]chan int) {\nfor k in m {\nvar d chan<- int = k\n_ = d\n}\n}").iter().any(|c| c == "HYB-E3012"));
    assert!(codes("fn f(m map[chan int]<-chan int) { for k in m { close(k) } }").is_empty());
    assert!(codes("fn f(s set[<-chan int]) { for ch in s { close(ch) } }").iter().any(|c| c == "HYB-E3009"));
    assert!(codes("fn f(s set[<-chan int]) {\nfor ch in s {\nvar d chan<- int = ch\n_ = d\n}\n}").iter().any(|c| c == "HYB-E3012"));
}
