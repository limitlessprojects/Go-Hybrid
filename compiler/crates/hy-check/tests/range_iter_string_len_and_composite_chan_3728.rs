//! Regression tests for #3728:
//! 1. `typed_int_bound` classifies `len(s)` on non-constant `string` operands
//!    as returning `int`, so `for i in lo..len(s)` and `for i in len(s)..u32(5)`
//!    with mismatched fixed-width integer bounds reject with `HYB-E2009`.
//! 2. `check_range_iterable` projects struct fields on composite bases
//!    (`boxes[0:1][0].ch`, `append(boxes, b)[0].ch`) when checking for
//!    send-only channels (`HYB-E2009`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test program must parse");
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn range_iter_rejects_runtime_string_len_mismatch_and_composite_send_only_chan_3728() {
    let cases = [
        "fn main() {\n    var lo i64 = 0\n    s := \"abc\"\n    for i in lo..len(s) {\n        _ = i\n    }\n}\n",
        "fn main() {\n    s := \"abc\"\n    for i in len(s)..u32(5) {\n        _ = i\n    }\n}\n",
        "struct box_ch { ch chan<- int }\nfn drain(boxes []box_ch) {\n    for v in boxes[0:1][0].ch {\n        _ = v\n    }\n}\nfn main() {}\n",
        "struct box_ch { ch chan<- int }\nfn drain(boxes []box_ch, b box_ch) {\n    for v in append(boxes, b)[0].ch {\n        _ = v\n    }\n}\nfn main() {}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E2009"),
            "expected HYB-E2009 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn range_iter_accepts_const_string_len_and_bidirectional_composite_field_chan_3728() {
    let src = "\
const cs = \"abc\"

struct box_ch {
    ch chan int
}

fn drain(boxes []box_ch) {
    for v in boxes[0:1][0].ch {
        _ = v
    }
}

fn main() {
    var lo i64 = 0
    for i in lo..len(cs) {
        _ = i
    }
    s := \"abc\"
    for j in 0..len(s) {
        _ = j
    }
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:?}",
        r.diagnostics
    );
}
