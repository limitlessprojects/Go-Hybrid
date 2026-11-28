use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn reject_recv_from_send_only_chan_through_pointer_field_and_composite_lits() {
    let cases = [
        (
            "untyped_slice_of_struct_lit_field",
            r#"
struct holder {
    ch chan<- int
}

fn bad_untyped_slice_of_struct_lit(ch chan<- int) {
    var a [<-([holder{ch: ch}][0].ch)]int
    _ = a
}
"#,
        ),
        (
            "pointer_field_on_composite_base",
            r#"
struct holder {
    ch chan<- int
}

fn bad_pointer_field_on_composite_base(ptrs []*holder, h *holder) {
    var a [<-(append(ptrs, h)[0].ch)]int
    _ = a
}
"#,
        ),
        (
            "sliced_untyped_slice_of_struct_lit_field",
            r#"
struct holder {
    ch chan<- int
}

fn bad_sliced_untyped_slice_of_struct_lit(ch chan<- int) {
    var a [<-( ([holder{ch: ch}][0:1])[0].ch )]int
    _ = a
}
"#,
        ),
        (
            "paren_degraded_some_unwrap",
            r#"
fn bad_paren_degraded_some_unwrap(chs []chan<- int, ch chan<- int) {
    var a [<-( (some(append(chs, ch)[0])).unwrap() )]int
    _ = a
}
"#,
        ),
    ];

    for (name, src) in cases {
        let parsed = parse_file("test.hy", src).expect("parse should succeed");
        let checked = check_file(&parsed);
        assert_eq!(
            checked.verdict,
            Verdict::Reject,
            "expected Reject for case {name}:\n{src}"
        );
        assert_eq!(
            checked.diagnostics.len(),
            1,
            "expected exactly 1 diagnostic for case {name}:\n{src}, got: {:?}",
            checked.diagnostics
        );
        assert_eq!(
            checked.diagnostics[0].code,
            "HYB-E2008",
            "expected HYB-E2008 diagnostic for case {name}:\n{src}, got: {:?}",
            checked.diagnostics
        );
    }
}
