use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn assert_reject_code(r: &hy_check::CheckResult, expected: &str, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.iter().any(|c| c == expected),
        "expected {} ({}), got {:?} (verdict={:?})",
        expected,
        what,
        codes,
        r.verdict
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject ({})", what);
}

#[test]
fn map_key_addr_of_o52_e3003() {
    let r = check(
        r#"
struct box {
    m map[string]int
}

fn worker(mp map[*box]int, done chan int) {
    for p, _ in mp {
        for i in 0..20 {
            p.m["k"] = i
        }
    }
    done <- 1
}

fn main() {
    b := box{m: map[string]int{"k": 0}}
    mp := map[*box]int{}
    mp[&b] = 1
    done := make(chan int, 1)
    task worker(move mp, done)
    for i in 0..20 {
        b.m["k"] = i
    }
    _ = <-done
}
"#,
    );
    assert_reject_code(&r, "HYB-E3003", "mp[&b] = 1 then move mp then b.m write");
}

#[test]
fn map_key_ptr_var_o52_e3003() {
    let r = check(
        r#"
struct box {
    m map[string]int
}

fn worker(mp map[*box]int, done chan int) {
    for p, _ in mp {
        p.m["k"] = 1
    }
    done <- 1
}

fn main() {
    b := box{m: map[string]int{"k": 0}}
    p := &b
    mp := map[*box]int{}
    mp[p] = 1
    done := make(chan int, 1)
    task worker(move mp, done)
    b.m["k"] = 2
    _ = <-done
}
"#,
    );
    assert_reject_code(&r, "HYB-E3003", "p := &b; mp[p] = 1 then move mp then b.m write");
}

#[test]
fn map_key_interproc_put_o52_e3003() {
    let r = check(
        r#"
struct box {
    m map[string]int
}

fn put(mp map[*box]int, p *box) {
    mp[p] = 1
}

fn worker(mp map[*box]int, done chan int) {
    for p, _ in mp {
        p.m["k"] = 1
    }
    done <- 1
}

fn main() {
    b := box{m: map[string]int{"k": 0}}
    mp := map[*box]int{}
    put(mp, &b)
    done := make(chan int, 1)
    task worker(move mp, done)
    b.m["k"] = 2
    _ = <-done
}
"#,
    );
    assert_reject_code(&r, "HYB-E3003", "interprocedural put(mp, &b) then move mp");
}

#[test]
fn map_key_const_backed_o54_e3004() {
    let r = check(
        r#"
const seed = map[string]int{"k": 1}

struct box {
    m map[string]int
}

fn worker(mp map[*box]int, done chan int) {
    for p, _ in mp {
        p.m["k"] = 2
    }
    done <- 1
}

fn main() {
    b := box{m: seed}
    mp := map[*box]int{}
    mp[&b] = 1
    done := make(chan int, 1)
    task worker(move mp, done)
    _ = <-done
}
"#,
    );
    assert_reject_code(&r, "HYB-E3004", "const-backed box stored via mp[&b] = 1");
}

#[test]
fn map_key_param_non_owned_e3004() {
    let r = check(
        r#"
struct box {
    m map[string]int
}

fn worker(mp map[*box]int, done chan int) {
    for p, _ in mp {
        p.m["k"] = 2
    }
    done <- 1
}

fn smuggle(b box, done chan int) {
    mp := map[*box]int{}
    mp[&b] = 1
    task worker(move mp, done)
}

fn main() {
    b := box{m: map[string]int{"k": 0}}
    done := make(chan int, 1)
    smuggle(b, done)
    b.m["k"] = 1
    _ = <-done
}
"#,
    );
    assert_reject_code(&r, "HYB-E3004", "borrowed parameter smuggled via mp[&b] = 1");
}

#[test]
fn map_key_local_borrow_escape_e3104() {
    let r = check(
        r#"
struct box {
    n int
}

fn leak() map[*box]int {
    b := box{n: 1}
    mp := map[*box]int{}
    mp[&b] = 1
    return mp
}

fn main() {
    _ = leak()
}
"#,
    );
    assert_reject_code(&r, "HYB-E3104", "return mp after mp[&b] = 1");
}

#[test]
fn map_key_compound_assign_o52_e3003() {
    let r = check(
        r#"
struct box {
    m map[string]int
}

fn worker(mp map[*box]int, done chan int) {
    for p, _ in mp {
        p.m["k"] = 1
    }
    done <- 1
}

fn main() {
    b := box{m: map[string]int{"k": 0}}
    mp := map[*box]int{}
    mp[&b] += 1
    done := make(chan int, 1)
    task worker(move mp, done)
    b.m["k"] = 2
    _ = <-done
}
"#,
    );
    assert_reject_code(&r, "HYB-E3003", "mp[&b] += 1 then move mp then b.m write");
}

#[test]
fn map_key_primitive_and_slice_index_controls_accept() {
    let r = check(
        r#"
fn worker(c map[int]int, d chan int) {
    _ = c[0]
    d <- 1
}

fn main() {
    m := map[string]int{"k": 1}
    mp := map[int]int{}
    i := 0
    mp[i] = 1
    mp[len(m)] = 2
    xs := []int{0, 0, 0}
    xs[len(m)] = 3
    done := make(chan int, 1)
    task worker(move mp, done)
    m["k"] = 9
    xs[0] = 4
    _ = <-done
}
"#,
    );
    let codes = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "primitive map key and slice index controls must accept, got {:?}",
        codes
    );
    assert!(codes.is_empty(), "expected 0 diagnostics, got {:?}", codes);
}
