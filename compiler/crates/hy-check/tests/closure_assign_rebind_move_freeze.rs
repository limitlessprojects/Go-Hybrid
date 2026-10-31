//! Regression tests for closure capture move-freeze (`HYB-E3003`, O5.1/O5.2)
//! across multi-value assignments (`m, tag = make_pair()`, `m, ok = buckets["a"]`,
//! `m, ok = <-ch`) in `rebind_assign_targets`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("test program failed to parse: {:?}", diags));
    check_file(&file)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn multival_call_assign_rebind_preserves_closure_capture_move_freeze_e3003() {
    let codes_call = error_codes(
        r#"
fn make_pair() (map[string]int, int) {
    return map[string]int{"k": 2}, 1
}

fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := map[string]int{"init": 0}
    var tag int = 0
    var f fn() = fn() {}
    if true {
        f = fn() {
            m["k"] = 99
        }
    }
    m, tag = make_pair()
    _ = tag
    task worker(move m, done)
    f()
    <-done
}
"#,
    );
    assert!(
        codes_call.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when multi-value call `m, tag = make_pair()` rebinds `m` before `move m` and `f()` runs, got {:?}",
        codes_call
    );

    let codes_alias = error_codes(
        r#"
fn id_pair(src map[string]int) (map[string]int, int) {
    return src, 1
}

fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := map[string]int{"init": 0}
    var tag int = 0
    var f fn() = fn() {}
    if true {
        f = fn() {
            m["k"] = 99
        }
    }
    m2 := map[string]int{"k": 2}
    m, tag = id_pair(m2)
    _ = tag
    task worker(move m2, done)
    f()
    <-done
}
"#,
    );
    assert!(
        codes_alias.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when multi-value `m, tag = id_pair(m2)` aliases `m` to moved `m2` and `f()` runs, got {:?}",
        codes_alias
    );
}

#[test]
fn comma_ok_map_and_chan_assign_rebind_preserve_closure_capture_move_freeze_e3003() {
    let codes_map = error_codes(
        r#"
fn worker(b map[string]map[string]int, done chan int) {
    b["a"] = map[string]int{"k": 1}
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    buckets := map[string]map[string]int{"a": map[string]int{"k": 2}}
    m := map[string]int{"init": 0}
    var ok bool = false
    var f fn() = fn() {}
    if true {
        f = fn() {
            m["k"] = 99
        }
    }
    m, ok = buckets["a"]
    _ = ok
    task worker(move buckets, done)
    f()
    <-done
}
"#,
    );
    assert!(
        codes_map.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when `m, ok = buckets[\"a\"]` rebinds captured `m` before `move buckets` and `f()` runs, got {:?}",
        codes_map
    );

    let codes_chan = error_codes(
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    ch := make(chan map[string]int, 1)
    ch <- move map[string]int{"k": 2}
    m := map[string]int{"init": 0}
    var ok bool = false
    var f fn() = fn() {}
    if true {
        f = fn() {
            m["k"] = 99
        }
    }
    m, ok = <-ch
    _ = ok
    task worker(move m, done)
    f()
    <-done
}
"#,
    );
    assert!(
        codes_chan.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when `m, ok = <-ch` rebinds captured `m` before `move m` and `f()` runs, got {:?}",
        codes_chan
    );
}

#[test]
fn closure_assigned_after_move_and_before_inner_shadow_rejects_e3003() {
    let codes = error_codes(
        r#"
fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := map[string]int{"init": 0}
    var f fn() = fn() {}
    task worker(move m, done)
    if true {
        f = fn() {
            m["k"] = 99
        }
        m := map[string]int{"shadow": 1}
        _ = m
    }
    f()
    <-done
}
"#,
    );
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when `f = fn() {{ m[\"k\"] = 99 }}` captures already-moved outer `m` before inner shadow `m := ...` and `f()` runs, got {:?}",
        codes
    );
}

#[test]
fn multival_assign_rebind_without_post_move_closure_call_still_accepts() {
    let file = parse_file(
        "test.hy",
        r#"
fn make_pair() (map[string]int, int) {
    return map[string]int{"k": 2}, 1
}

fn worker(m map[string]int, done chan int) {
    m["k"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := map[string]int{"init": 0}
    var tag int = 0
    m, tag = make_pair()
    _ = tag
    task worker(move m, done)
    <-done
}
"#,
    )
    .expect("parse");
    let res = check_file(&file);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept when `m, tag = make_pair()` is moved to worker with no post-move closure use, got {:?}",
        res.diagnostics
    );
}
