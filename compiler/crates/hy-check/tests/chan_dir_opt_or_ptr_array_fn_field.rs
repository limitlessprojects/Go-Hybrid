use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn option_or_rejects_mismatched_channel_direction_arg() {
    let src = "\
fn bad_or_dir(opt option[chan<- int], rx <-chan int) chan<- int {
    return opt.or(rx)
}
fn bad_or_narrow_to_bidir(opt option[chan int], rx <-chan int) chan int {
    return opt.or(rx)
}
fn main() {}
";
    let found = codes(src);
    let e3012 = found.iter().filter(|c| c.as_str() == "HYB-E3012").count();
    assert!(e3012 >= 2, "expected >= 2 HYB-E3012 diagnostics, got {found:?}");
}

#[test]
fn option_or_accepts_bidir_narrowing_arg() {
    let src = "\
fn ok_or(opt option[<-chan int], bi chan int) <-chan int {
    return opt.or(bi)
}
fn main() {}
";
    let found = codes(src);
    assert!(found.is_empty(), "expected clean check, got {found:?}");
}

#[test]
fn ptr_to_array_preserves_channel_direction_across_index_slice_place_and_range() {
    let src = "\
fn bad_index_send(p *[2]<-chan int) {
    p[0] <- 1
}
fn bad_index_close(p *[2]<-chan int) {
    close(p[1])
}
fn bad_place_assign(p *[2]chan<- int, rx <-chan int) {
    p[0] = rx
}
fn bad_slice_send(p *[2]<-chan int) {
    s := p[:]
    s[0] <- 1
}
fn bad_range_send(p *[2]<-chan int) {
    for _, ch in p {
        ch <- 1
    }
}
fn main() {}
";
    let found = codes(src);
    let e3008 = found.iter().filter(|c| c.as_str() == "HYB-E3008").count();
    let e3009 = found.iter().filter(|c| c.as_str() == "HYB-E3009").count();
    let e3012 = found.iter().filter(|c| c.as_str() == "HYB-E3012").count();
    assert!(e3008 >= 3, "expected >= 3 HYB-E3008 diagnostics, got {found:?}");
    assert!(e3009 >= 1, "expected >= 1 HYB-E3009 diagnostic, got {found:?}");
    assert!(e3012 >= 1, "expected >= 1 HYB-E3012 diagnostic, got {found:?}");
}

#[test]
fn multi_return_fn_field_preserves_channel_direction_issue_1358() {
    let src = "\
struct holder {
    f fn() int <-chan int
}
fn bad_fn_field_close(h holder) {
    _, rx := h.f()
    close(rx)
}
fn bad_fn_field_send(h holder) {
    _, rx := h.f()
    rx <- 1
}
fn main() {}
";
    let found = codes(src);
    assert!(found.iter().any(|c| c == "HYB-E3009"), "expected HYB-E3009, got {found:?}");
    assert!(found.iter().any(|c| c == "HYB-E3008"), "expected HYB-E3008, got {found:?}");
}

#[test]
fn user_pop_method_on_slice_alias_preserves_declared_return_channel_direction() {
    let src = "\
type pipe_stack = []chan<- int
fn (s pipe_stack) pop() <-chan int {
    ch := make(chan int, 1)
    return ch
}
fn bad_alias_pop(s pipe_stack) {
    rx := s.pop()
    rx <- 1
}
fn main() {}
";
    let found = codes(src);
    assert!(found.iter().any(|c| c == "HYB-E3008"), "expected HYB-E3008, got {found:?}");
}
