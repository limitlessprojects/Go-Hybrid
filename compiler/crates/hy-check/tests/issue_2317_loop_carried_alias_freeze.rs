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

const REPRO_2317: &str = "\
package main

fn wm(x map[string]int, done chan int) {
    for i in 0..1000 {
        x[\"race\"] = i
    }
    done <- 1
}
fn main() {
    done := make(chan int, 2)
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 0..2 {
        t := nxt
        nxt = map[string]int{}
        task wm(move t, done)
        keep[\"race\"] = i
        keep = nxt
    }
    <-done
    <-done
}
";

#[test]
fn test_loop_carried_tail_alias_freeze_is_rejected_e3003() {
    let r = check(REPRO_2317);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for write through live alias of value moved on next iteration, got: {:?}",
        cs
    );
}

#[test]
fn test_channel_send_variant_is_rejected_e3003() {
    let src = "\
package main

fn main() {
    ch := make(chan map[string]int, 2)
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 0..2 {
        t := nxt
        nxt = map[string]int{}
        ch <- move t
        keep[\"race\"] = i
        keep = nxt
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for channel send move alias write, got: {:?}",
        cs
    );
}

#[test]
fn test_slice_shared_value_variant_is_rejected_e3003() {
    let src = "\
package main

fn ws(x []int, done chan int) {
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    keep := []int{0}
    nxt := []int{0}
    for i in 0..2 {
        t := nxt
        nxt = []int{0}
        task ws(move t, done)
        keep[0] = i
        keep = nxt
    }
    <-done
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for slice alias write after move, got: {:?}",
        cs
    );
}

#[test]
fn test_condition_loop_variant_is_rejected_e3003() {
    let src = "\
package main

fn wm(x map[string]int) {}

fn main() {
    keep := map[string]int{}
    nxt := map[string]int{}
    i := 0
    for i < 2 {
        t := nxt
        nxt = map[string]int{}
        task wm(move t)
        keep[\"race\"] = i
        keep = nxt
        i = i + 1
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for condition loop alias write, got: {:?}",
        cs
    );
}

#[test]
fn test_tail_alias_in_if_branch_is_rejected_e3003() {
    let src = "\
package main

fn wm(x map[string]int) {}

fn main() {
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 0..2 {
        t := nxt
        nxt = map[string]int{}
        task wm(move t)
        keep[\"race\"] = i
        if true {
            keep = nxt
        }
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for if-tail alias write, got: {:?}",
        cs
    );
}

#[test]
fn test_helper_fn_variant_is_rejected_e3003() {
    let src = "\
package main

fn wm(x map[string]int) {}

fn run_worker() {
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 0..2 {
        t := nxt
        nxt = map[string]int{}
        task wm(move t)
        keep[\"race\"] = i
        keep = nxt
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 in helper fn, got: {:?}",
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

fn main() {
    done := make(chan int, 2)
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 0..1 {
        t := nxt
        nxt = map[string]int{}
        task wm(move t, done)
        keep[\"race\"] = i
        keep = nxt
    }
    <-done
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

fn main() {
    done := make(chan int, 2)
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 0..2 {
        t := nxt
        nxt = map[string]int{}
        task wm(move t, done)
        keep[\"race\"] = i
        keep = nxt
        break
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics for sound trailing break control, got: {:?}",
        cs
    );
}
