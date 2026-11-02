use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_src(src: &str) -> Vec<(String, String)> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code, d.message))
        .collect()
}

#[test]
fn range_over_send_only_channel_via_receive_loop_var_and_aliased_pointer_field_rejected() {
    // 1. Ranging over `<-ch_of_tx` where `ch_of_tx` yields a send-only `chan<- int`
    let src_recv = r#"
package main

type tx_chan = chan<- int

fn bad_recv(ch_of_tx <-chan tx_chan) {
    for x in <-ch_of_tx {
        _ = x
    }
}
"#;
    let diags = check_src(src_recv);
    assert!(
        diags
            .iter()
            .any(|(code, msg)| code == "HYB-E2009" && msg.contains("send-only")),
        "expected HYB-E2009 for ranging over received send-only channel, got: {:?}",
        diags
    );

    // 2. Ranging over loop variable `ch` bound from `for ch in txs` (`[]chan<- int`)
    let src_loop_var = r#"
package main

fn bad_loop_var(txs []chan<- int) {
    for ch in txs {
        for x in ch {
            _ = x
        }
    }
}
"#;
    let diags = check_src(src_loop_var);
    assert!(
        diags
            .iter()
            .any(|(code, msg)| code == "HYB-E2009" && msg.contains("send-only")),
        "expected HYB-E2009 for ranging over loop-bound send-only channel, got: {:?}",
        diags
    );

    // 3. Ranging over field `s.ch` on a pointer to a type-aliased struct `*holder_alias`
    let src_alias_ptr_field = r#"
package main

struct holder {
    ch chan<- int
}

type holder_alias = holder

fn bad_field(s *holder_alias) {
    for x in s.ch {
        _ = x
    }
}
"#;
    let diags = check_src(src_alias_ptr_field);
    assert!(
        diags
            .iter()
            .any(|(code, msg)| code == "HYB-E2009" && msg.contains("send-only")),
        "expected HYB-E2009 for ranging over send-only channel field via pointer to type alias, got: {:?}",
        diags
    );
}

#[test]
fn range_bounds_with_compound_and_chained_non_integer_consts_rejected() {
    let src = r#"
package main

const b_not = !true
const b_cmp = 1 < 2
const f_neg = -1.5
const f_sum = 1.5 + 2.5
const s_cat = "a" + "b"
const f_chain = f_neg

fn bad_const_bounds() {
    for i in 0..b_not {
        _ = i
    }
    for j in 0..b_cmp {
        _ = j
    }
    for k in 0..f_neg {
        _ = k
    }
    for m in 0..f_sum {
        _ = m
    }
    for n in 0..s_cat {
        _ = n
    }
    for p in 0..f_chain {
        _ = p
    }
}
"#;
    let diags = check_src(src);
    let e2009_count = diags
        .iter()
        .filter(|(code, msg)| code == "HYB-E2009" && msg.contains("is not an integer"))
        .count();
    assert_eq!(
        e2009_count, 6,
        "expected 6 HYB-E2009 diagnostics for compound/chained non-integer const bounds, got: {:?}",
        diags
    );
}

#[test]
fn range_bounds_with_unary_and_shift_fixed_width_integers_checked_for_mismatch_and_overflow() {
    let src = r#"
package main

fn bad_unary_and_shift(x int8, u u32) {
    for i in 0..(-x) {
        _ = i
    }
    for j in 0..(u << 1) {
        if j > 0 {
            break
        }
    }
    for m in (u << 1)..=10 {
        if m > 0 {
            break
        }
    }
    for k in (-x)..300 {
        _ = k
    }
}
"#;
    let diags = check_src(src);
    assert!(
        diags
            .iter()
            .any(|(code, msg)| code == "HYB-E2009"
                && msg.contains("-x")
                && msg.contains("mismatched integer types")),
        "expected HYB-E2009 mismatch for 0..(-x), got: {:?}",
        diags
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| code == "HYB-E2009"
                && msg.contains("u << 1")
                && msg.contains("mismatched integer types")),
        "expected HYB-E2009 mismatch for 0..(u << 1) and (u << 1)..=10, got: {:?}",
        diags
    );
    assert!(
        diags
            .iter()
            .any(|(code, msg)| code == "HYB-E2009"
                && msg.contains("300")
                && msg.contains("overflows the range counter's type `int8`")),
        "expected HYB-E2009 overflow for (-x)..300, got: {:?}",
        diags
    );
}
