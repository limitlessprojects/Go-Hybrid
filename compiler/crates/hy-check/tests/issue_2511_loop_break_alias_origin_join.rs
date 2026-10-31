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
fn test_i01_break_exit_non_owned_origin_rejects_e3004() {
    let src = "\
package main

fn wm(x map[string]int, done chan int) {
    for i in 0..1000 {
        x[\"race\"] = i
    }
    done <- 1
}

fn spawn2(x map[string]int, done chan int) {
    cur := map[string]int{}
    for j in 0..2 {
        cur = x
        if j == 1 {
            break
        }
        cur = map[string]int{}
    }
    task wm(move cur, done)
}

fn main() {
    m := map[string]int{\"a\": 1}
    done := make(chan int, 1)
    spawn2(m, done)
    for i in 0..1000 {
        m[\"a\"] = i
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving non-owned origin cur aliased on break path, got: {:?}",
        cs
    );
}

#[test]
fn test_i02_break_exit_alias_freeze_rejects_e3003() {
    let src = "\
package main

fn wm(x map[string]int, done chan int) {
    for i in 0..1000 {
        x[\"race\"] = i
    }
    done <- 1
}

fn main() {
    m := map[string]int{\"a\": 1}
    a := map[string]int{}
    done := make(chan int, 1)
    for j in 0..2 {
        a = m
        if j == 1 {
            break
        }
        a = map[string]int{}
    }
    task wm(move m, done)
    for i in 0..1000 {
        a[\"a\"] = i
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for writing through alias a of moved m preserved via break, got: {:?}",
        cs
    );
}

#[test]
fn test_i03_break_exit_chan_send_move_freeze_rejects_e3003() {
    let src = "\
package main

fn consumer(ch chan map[string]int, done chan int) {
    x := <-ch
    for i in 0..1000 {
        x[\"race\"] = i
    }
    done <- 1
}

fn main() {
    m := map[string]int{\"a\": 1}
    a := map[string]int{}
    ch := make(chan map[string]int, 1)
    done := make(chan int, 1)
    task consumer(ch, done)
    for j in 0..2 {
        a = m
        if j == 1 {
            break
        }
        a = map[string]int{}
    }
    ch <- move m
    for i in 0..1000 {
        a[\"a\"] = i
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for writing through alias a of chan-sent moved m preserved via break, got: {:?}",
        cs
    );
}

#[test]
fn test_l1_break_exit_const_mutates_param_rejects_e3006() {
    let src = "\
package main

const tags = map[string]int{\"a\": 1}

fn reader(done chan int) {
    s := 0
    for i in 0..3000 {
        s = s + tags[\"a\"]
    }
    _ = s
    done <- 1
}

fn touch(c map[string]int, v int) {
    t := map[string]int{}
    for i in 0..2 {
        t = c
        if i >= 0 {
            break
        }
        t = map[string]int{}
    }
    t[\"a\"] = v
}

fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..3000 {
        touch(tags, i)
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper touches const tags via alias preserved across break, got: {:?}",
        cs
    );
}

#[test]
fn test_l2_break_exit_param_stores_freeze_rejects_e3003() {
    let src = "\
package main

fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    tmp := other
    for i in 0..2 {
        tmp = src
        if i >= 0 {
            break
        }
        tmp = other
    }
    dst[0] = tmp
}

fn worker(s []map[string]int, done chan int) {
    for i in 0..1000 {
        s[0][\"race\"] = i
    }
    done <- 1
}

fn main() {
    m := map[string]int{\"a\": 1}
    o := map[string]int{\"b\": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    for i in 0..1000 {
        m[\"a\"] = i
    }
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when caller writes m after moving dst containing m stored via break alias, got: {:?}",
        cs
    );
}

#[test]
fn test_sound_loop_break_with_owned_value_stays_accepted() {
    let src = "\
package main

fn wm(x map[string]int, done chan int) {
    done <- 1
}

fn main() {
    cur := map[string]int{\"k\": 1}
    done := make(chan int, 1)
    for i in 0..2 {
        if i == 1 {
            break
        }
    }
    task wm(move cur, done)
    <-done
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected 0 errors for sound owned move after loop break, got: {:?}",
        cs
    );
}

