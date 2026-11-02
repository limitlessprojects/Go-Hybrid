use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_octal_bound_three_iterations_is_rejected_e3003() {
    // 010 in octal is 8. 8..11 has 3 iterations (8, 9, 10).
    // Bypassing backedge check would cause data races.
    let src = "\
package main

fn wm(x map[string]int, done chan int) {
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 010..11 {
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
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for octal 010..11 (3 iterations), got: {:?}",
        cs
    );
}

#[test]
fn test_octal_bound_single_iteration_is_accepted() {
    // 010..011 in octal is 8..9, exactly 1 iteration, unmutated.
    let src = "\
package main

fn wm(x map[string]int, done chan int) {
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 010..011 {
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
        "expected no errors for single iteration octal 010..011, got: {:?}",
        cs
    );
}

#[test]
fn test_hex_and_bin_bounds_multi_iteration_are_rejected_e3003() {
    let src_hex = "\
package main

fn wm(x map[string]int, done chan int) {
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    keep := map[string]int{}
    nxt := map[string]int{}
    for i in 0x0..0x2 {
        t := nxt
        nxt = map[string]int{}
        task wm(move t, done)
        keep[\"race\"] = i
        keep = nxt
    }
    <-done
}
";
    let r_hex = check(src_hex);
    let cs_hex = error_codes(&r_hex);
    assert!(
        cs_hex.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for 0x0..0x2, got: {:?}",
        cs_hex
    );
}

#[test]
fn test_loop_var_mutation_rejection_e3003() {
    // Range is statically 0..1, but induction variable i is mutated in the body (i = -1).
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
        i = -1
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when loop induction variable is mutated, got: {:?}",
        cs
    );
}

#[test]
fn test_loop_var_decrement_rejection_e3003() {
    // Range is statically 0..1, but induction variable i is decremented in the body (i--).
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
        i--
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when loop induction variable is decremented, got: {:?}",
        cs
    );
}

#[test]
fn test_unrelated_var_mutation_in_single_iteration_is_accepted() {
    // Range is statically 0..1, unrelated local variable is mutated. Loop induction var i is intact.
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
        temp := 0
        temp = 1
        keep[\"temp\"] = temp
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected no errors when unrelated var is mutated in 0..1 loop, got: {:?}",
        cs
    );
}
