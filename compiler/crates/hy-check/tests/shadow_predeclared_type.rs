use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn struct_shadowing_rune_with_map_rejected_across_tasks_issue_326() {
    let src = r#"
struct rune {
    m map[string]string
}
fn worker(h rune, id string, done chan<- string) {
    h.m[id] = id
    done <- id
}
fn main() {
    h := rune{ m: map[string]string{} }
    done := make(chan string)
    task worker(h, "a", done)
    task worker(h, "b", done)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn struct_shadowing_string_with_slice_rejected_across_tasks_issue_326() {
    let src = r#"
struct string {
    s []int
}
fn worker(h string, done chan<- bool) {
    done <- true
}
fn main() {
    h := string{ s: [1, 2, 3] }
    done := make(chan bool)
    task worker(h, done)
    task worker(h, done)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn positive_control_standard_primitive_rune_accepted_across_tasks() {
    let src = r#"
fn worker(r rune, done chan<- bool) {
    done <- true
}
fn main() {
    r := 'a'
    done := make(chan bool)
    task worker(r, done)
    task worker(r, done)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn type_alias_shadowing_predeclared_with_map_rejected_across_tasks() {
    let src = r#"
type my_map = map[string]string
type rune = my_map
fn worker(m rune, done chan<- bool) {
    done <- true
}
fn main() {
    m := map[string]string{}
    done := make(chan bool)
    task worker(m, done)
    task worker(m, done)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3001" || d.code == "HYB-E3005"),
        "expected rejection diagnostic, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn cyclic_type_alias_fails_closed_issue_326() {
    let src = r#"
type a = b
type b = a
fn worker(x a, done chan<- bool) {
    done <- true
}
fn main() {
    var x a
    done := make(chan bool)
    task worker(x, done)
    task worker(x, done)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "cyclic type alias must fail closed, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn nested_shadowed_struct_fields_rejected_across_tasks() {
    let src = r#"
struct string {
    data map[int]int
}
struct rune {
    s string
}
fn worker(h rune, done chan<- bool) {
    done <- true
}
fn main() {
    h := rune{ s: string{ data: map[int]int{} } }
    done := make(chan bool)
    task worker(h, done)
    task worker(h, done)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn method_receiver_on_user_struct_shadowing_rune_accepted() {
    let src = r#"
struct rune {
    count int
}
fn (r rune) get() int {
    return r.count
}
fn main() {
    r := rune{ count: 10 }
    fmt.println(r.get())
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn method_receiver_on_type_alias_rejected_e2007() {
    let src = r#"
type my_int = int
fn (m my_int) bad() int {
    return 1
}
fn main() {
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E2007"),
        "expected HYB-E2007, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn method_receiver_on_primitive_rune_rejected_e2007() {
    let src = r#"
fn (r rune) bad() int {
    return 1
}
fn main() {
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E2007"),
        "expected HYB-E2007, got: {:?}",
        r.diagnostics
    );
}
