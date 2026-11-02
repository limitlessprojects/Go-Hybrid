//! Regression tests for Issue #3137: user enum values are typed from the
//! enum declaration — variant constructor payload type and arity, and the
//! type of a `match` payload binding (`circle(r)` / `shape.circle(r)`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn e1002_count(src: &str) -> usize {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error && d.code == "HYB-E1002")
        .count()
}

#[test]
fn enum_payload_type_and_arity_errors_rejected() {
    for (name, src) in [
        (
            "r1",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.circle("x")
    fmt.println(s)
}
"#,
        ),
        (
            "r2",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.rectangle(1.0)
    fmt.println(s)
}
"#,
        ),
        (
            "r3",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.rectangle("h", "i")
    fmt.println(s)
}
"#,
        ),
        (
            "r3b",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.point(1)
    fmt.println(s)
}
"#,
        ),
        (
            "r3c",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.circle()
    fmt.println(s)
}
"#,
        ),
        (
            "r4",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.circle(1.5)
    match s {
        circle(r): { var x int = r
            fmt.println(x) }
        rectangle(a): fmt.println(a)
        point: fmt.println("p")
    }
}
"#,
        ),
        (
            "r5",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.circle(1.5)
    match s {
        circle(r): fmt.println(r + "x")
        _: fmt.println("o")
    }
}
"#,
        ),
        (
            "r5b",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.rectangle("q")
    match s {
        shape.rectangle(a): { var n int = a
            fmt.println(n) }
        _: fmt.println("o")
    }
}
"#,
        ),
    ] {
        assert!(e1002_count(src) >= 1, "{name}: expected HYB-E1002");
    }
}

#[test]
fn well_typed_enum_payloads_and_bindings_accepted() {
    let src = r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}

fn main() {
    s := shape.rectangle("q")
    t := shape.circle(2)
    u := shape.circle(2.5)
    v := shape.point
    fmt.println(t, u, v)
    match s {
        circle(r): { var f f64 = r
            fmt.println(f + 1.0) }
        rectangle(a): fmt.println(a + "!")
        point: fmt.println("p")
    }
    o := some(3)
    match o {
        some(v): fmt.println(v + 1)
        none: fmt.println("n")
    }
}
"#;
    assert_eq!(e1002_count(src), 0);
}

#[test]
fn ambiguous_bare_variant_pattern_stays_fail_open() {
    // `item(v)` names a variant in two enums with different payload types,
    // so the bare pattern does not pick one.
    let src = r#"
enum a {
    item(int)
    none_a
}
enum b {
    item(string)
    none_b
}
fn main() {
    x := b.item("s")
    match x {
        item(v): fmt.println(v)
        _: fmt.println("o")
    }
}
"#;
    assert_eq!(e1002_count(src), 0);
}

#[test]
fn enum_values_only_fit_their_own_enum_and_have_no_fields() {
    for (name, src) in [
        (
            "r7",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}
fn (c color) name() string { return "c" }

fn main() {
    var c color = shape.point
    fmt.println(c)
}
"#,
        ),
        (
            "r8",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}
fn (c color) name() string { return "c" }

fn main() {
    c := color.red
    var n int = c
    fmt.println(n)
}
"#,
        ),
        (
            "r8b",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}
fn (c color) name() string { return "c" }

fn main() {
    var n string = color.green
    fmt.println(n)
}
"#,
        ),
        (
            "r10",
            r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}
fn (c color) name() string { return "c" }

fn main() {
    s := shape.circle(2.0)
    fmt.println(s.radius)
}
"#,
        ),
    ] {
        assert_eq!(e1002_count(src), 1, "{name} must be rejected with one HYB-E1002");
    }
}

#[test]
fn enum_value_to_own_type_any_and_method_accepted() {
    let src = r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}
fn (c color) name() string { return "c" }

fn main() {
    c := color.red
    var d color = c
    var a any = c
    fmt.println(d, a, c.name())
}
"#;
    assert_eq!(e1002_count(src), 0);
}
