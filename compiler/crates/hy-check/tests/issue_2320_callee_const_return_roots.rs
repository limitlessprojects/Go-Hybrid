//! issue_2320_callee_const_return_roots.rs — soundness test for issue #2320:
//! collect_callee_const_return_roots fails open at depth > 4 and on fn-value
//! callees, allowing a const map to reach a task (data race) or mutate a const.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_2320.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str, what: &str, expected_codes: &[&str]) {
    let r = check(src);
    let cs = codes(&r);
    for &code in expected_codes {
        assert!(
            cs.iter().any(|c| c == code),
            "{}: expected diagnostic {}, got {:?}",
            what,
            code,
            cs
        );
    }
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

fn assert_accept(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{}: got {:?}", what, codes(&r));
}

#[test]
fn program_a_deep_const_return_through_5_wrappers_is_rejected() {
    let src = r#"
package main

const shared = map[string]int{"a": 1}

fn leaf() map[string]int { return shared }
fn h1() map[string]int { return leaf() }
fn h2() map[string]int { return h1() }
fn h3() map[string]int { return h2() }
fn h4() map[string]int { return h3() }
fn h5() map[string]int { return h4() }

fn worker(m map[string]int, done chan int) {
    _ = m["a"]
    done <- 1
}

fn main() {
    m := h5()
    done := make(chan int, 1)
    task worker(move m, done)
    m2 := h5()
    m2["a"] = 99
    <-done
}
"#;
    assert_reject(
        src,
        "deep const return through 5 wrappers reaches task and mutates const",
        &["HYB-E3004", "HYB-E3006"],
    );
}

#[test]
fn program_b_indirect_const_return_through_fn_value_is_rejected() {
    let src = r#"
package main

const shared = map[string]int{"a": 1}

fn leaf() map[string]int { return shared }

fn worker(m map[string]int, done chan int) {
    _ = m["a"]
    done <- 1
}

fn main() {
    cb := leaf
    m := cb()
    done := make(chan int, 1)
    task worker(move m, done)
    m2 := cb()
    m2["a"] = 99
    <-done
}
"#;
    assert_reject(
        src,
        "indirect const return through fn value reaches task and mutates const",
        &["HYB-E3004", "HYB-E3006"],
    );
}

#[test]
fn control_direct_leaf_const_return_is_rejected() {
    let src = r#"
package main

const shared = map[string]int{"a": 1}

fn leaf() map[string]int { return shared }

fn worker(m map[string]int, done chan int) {
    _ = m["a"]
    done <- 1
}

fn main() {
    m := leaf()
    done := make(chan int, 1)
    task worker(move m, done)
    m2 := leaf()
    m2["a"] = 99
    <-done
}
"#;
    assert_reject(
        src,
        "direct leaf call with const return rejects",
        &["HYB-E3004", "HYB-E3006"],
    );
}

#[test]
fn control_deep_fresh_map_is_accepted() {
    let src = r#"
package main

fn fresh() map[string]int { return map[string]int{"a": 1} }
fn h1() map[string]int { return fresh() }
fn h2() map[string]int { return h1() }
fn h3() map[string]int { return h2() }
fn h4() map[string]int { return h3() }
fn h5() map[string]int { return h4() }

fn worker(m map[string]int, done chan int) {
    _ = m["a"]
    done <- 1
}

fn main() {
    m := h5()
    done := make(chan int, 1)
    task worker(move m, done)
    <-done
}
"#;
    assert_accept(src, "deep fresh map through 5 wrappers is safe to move");
}

#[test]
fn control_recursive_fresh_map_cycle_guard_is_accepted() {
    let src = r#"
package main

const shared = map[string]int{"a": 1}

fn recurse(n int) map[string]int {
    if n <= 0 {
        return map[string]int{"ok": 1}
    }
    return recurse(n - 1)
}

fn worker(m map[string]int, done chan int) {
    _ = m["ok"]
    done <- 1
}

fn main() {
    m := recurse(5)
    done := make(chan int, 1)
    task worker(move m, done)
    <-done
}
"#;
    assert_accept(src, "recursive fresh map is not tainted by cycle guard");
}

#[test]
fn control_fn_value_no_file_consts_is_accepted() {
    let src = r#"
package main

fn fresh() map[string]int { return map[string]int{"a": 1} }

fn worker(m map[string]int, done chan int) {
    _ = m["a"]
    done <- 1
}

fn main() {
    cb := fresh
    m := cb()
    done := make(chan int, 1)
    task worker(move m, done)
    <-done
}
"#;
    assert_accept(src, "fn value without file consts in file is safe to move");
}

#[test]
fn control_external_package_call_is_not_const_tainted() {
    let src = r#"
package main

import "strings"

const shared = map[string]int{"a": 1}

fn worker(parts []string, done chan int) {
    _ = parts[0]
    done <- 1
}

fn main() {
    parts := strings.split("a,b,c", ",")
    done := make(chan int, 1)
    task worker(move parts, done)
    <-done
}
"#;
    assert_accept(src, "external package call strings.Split is safe to move");
}
