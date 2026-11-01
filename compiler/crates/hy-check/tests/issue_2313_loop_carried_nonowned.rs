use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// Repro from issue #2313:
// A non-owned parameter is stored into an outer binding (`cur = x`) at the loop tail.
// On the next iteration, `t := cur` copies that origin, and `task wm(move t, done)`
// attempts to move it across a task boundary.
const REPRO_2313: &str = "\
package main

fn wm(x map[string]int, done chan int) {
    for i in 0..1000 {
        x[\"race\"] = i
    }
    done <- 1
}

fn spawn2(x map[string]int, done chan int) {
    cur := map[string]int{}
    for i in 0..2 {
        t := cur
        cur = map[string]int{}
        task wm(move t, done)
        cur = x
    }
}

fn main() {
    m := map[string]int{\"init\": 0}
    done := make(chan int, 2)
    spawn2(m, done)
    for i in 0..1000 {
        m[\"race\"] = i
    }
    <-done
    <-done
}
";

#[test]
fn test_loop_carried_non_owned_param_move_is_rejected_e3004() {
    let r = check(REPRO_2313);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for loop-carried non-owned parameter move, got: {:?}",
        cs
    );
}

#[test]
fn test_channel_send_move_is_rejected_e3004() {
    let src = "\
package main

fn spawn(x map[string]int, ch chan map[string]int) {
    cur := map[string]int{}
    for i in 0..2 {
        t := cur
        cur = map[string]int{}
        ch <- move t
        cur = x
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for loop-carried channel send move, got: {:?}",
        cs
    );
}

#[test]
fn test_condition_loop_move_is_rejected_e3004() {
    let src = "\
package main

fn wm(x map[string]int) {}

fn spawn(x map[string]int) {
    cur := map[string]int{}
    i := 0
    for i < 2 {
        t := cur
        cur = map[string]int{}
        task wm(move t)
        cur = x
        i = i + 1
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for condition-loop carried non-owned move, got: {:?}",
        cs
    );
}

#[test]
fn test_select_arm_send_move_is_rejected_e3004() {
    let src = "\
package main

fn spawn(x map[string]int, ch chan map[string]int) {
    cur := map[string]int{}
    for i in 0..2 {
        t := cur
        cur = map[string]int{}
        select {
        ch <- move t:
            cur = map[string]int{}
        }
        cur = x
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for select-arm carried non-owned move, got: {:?}",
        cs
    );
}

#[test]
fn test_nested_loop_move_is_rejected_e3004() {
    let src = "\
package main

fn wm(x map[string]int) {}

fn spawn(x map[string]int) {
    cur := map[string]int{}
    for i in 0..2 {
        t := cur
        cur = map[string]int{}
        task wm(move t)
        for j in 0..1 {
            cur = x
        }
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for nested loop assigned non-owned move, got: {:?}",
        cs
    );
}

#[test]
fn test_single_iteration_control_0_to_1_is_accepted() {
    let src = "\
package main

fn wm(x map[string]int, done chan int) {
    done <- 1
}

fn spawn2(x map[string]int, done chan int) {
    cur := map[string]int{}
    for i in 0..1 {
        t := cur
        cur = map[string]int{}
        task wm(move t, done)
        cur = x
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics for sound 0..1 loop control, got: {:?}",
        cs
    );
}

#[test]
fn test_trailing_break_control_is_accepted() {
    let src = "\
package main

fn wm(x map[string]int, done chan int) {
    done <- 1
}

fn spawn2(x map[string]int, done chan int) {
    cur := map[string]int{}
    for i in 0..2 {
        t := cur
        cur = map[string]int{}
        task wm(move t, done)
        cur = x
        break
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics for sound trailing break loop control, got: {:?}",
        cs
    );
}
