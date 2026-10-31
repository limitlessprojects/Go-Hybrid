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

fn check_loop_body(rng: &str, body: &str, expect_e3003: bool) {
    let src = format!(
        "package main\nfn wm(x map[string]int, done chan int) {{ done <- 1 }}\nfn main() {{\n    done := make(chan int, 2)\n    keep := map[string]int{{}}\n    nxt := map[string]int{{}}\n    for i in {rng} {{\n        t := nxt\n        nxt = map[string]int{{}}\n        task wm(move t, done)\n        keep[\"race\"] = i\n        keep = nxt\n{body}\n    }}\n    <-done\n}}\n"
    );
    let cs = error_codes(&check(&src));
    if expect_e3003 {
        assert!(cs.iter().any(|c| c == "HYB-E3003"), "expected HYB-E3003 for {rng} with {body}, got: {cs:?}");
    } else {
        assert!(cs.is_empty(), "expected 0 diagnostics for {rng} with {body}, got: {cs:?}");
    }
}

#[test]
fn test_single_iteration_radix_and_empty_ranges_accepted() {
    for rng in ["0..01", "0..0o1", "0x0..0x1", "1..0"] {
        check_loop_body(rng, "", false);
    }
}

#[test]
fn test_multi_iteration_radix_ranges_rejected() {
    for rng in ["0..0o2", "0b0..0b10", "010..11"] {
        check_loop_body(rng, "", true);
    }
}

#[test]
fn test_mutated_induction_var_direct_assign_is_rejected_e3003() {
    check_loop_body("0..1", "        i = -1", true);
}

#[test]
fn test_mutated_induction_var_sub_assign_is_rejected_e3003() {
    check_loop_body("0..1", "        i -= 1", true);
}

#[test]
fn test_mutated_induction_var_dec_op_is_rejected_e3003() {
    check_loop_body("0..1", "        i--", true);
}

#[test]
fn test_mutated_induction_var_closure_capture_is_rejected_e3003() {
    check_loop_body("0..1", "        cb := fn() { i = -1 }\n        cb()", true);
}

#[test]
fn test_mutated_induction_var_target_index_is_rejected_e3003() {
    check_loop_body("0..1", "        arr := []int{0}\n        arr[fn() int { i = -1; return 0 }()] = 1", true);
}

#[test]
fn test_mutated_induction_var_select_recv_is_rejected_e3003() {
    check_loop_body("0..1", "        ch := make(chan int, 1)\n        select { i := <-ch: }", true);
}

#[test]
fn test_shadowed_induction_var_in_closure_is_accepted() {
    check_loop_body("0..1", "        f := fn(i int) { i = i + 1 }\n        f(10)", false);
}
