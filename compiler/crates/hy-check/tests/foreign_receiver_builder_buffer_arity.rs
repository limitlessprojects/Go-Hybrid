//! Regression: `strings.builder` and `bytes.buffer` receiver methods must be
//! arity-checked at `hyb check` time (Row 13 BUILDABLE, HYB-E1002).
//!
//! `foreign_method_surface` rejected method NAMES outside the curated
//! surface, but every blessed name was accepted with any argument count or
//! a `...` spread. `b.write_string("a", "b")`, `buf.truncate()` and
//! `b.len(3)` all passed `hyb check` with 0 diagnostics, then died at
//! `go build`:
//!
//! ```text
//! too many arguments in call to b.WriteString
//! not enough arguments in call to buf.Truncate
//! cannot use ... in call to non-variadic sb.WriteString
//! ```
//!
//! The reject cases pin the fail-closed arity check; the accept control uses
//! every modelled method at its Go arity so the check never over-rejects.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn builder_prog(call: &str) -> String {
    format!("fn main() {{\n    var b strings.builder\n    {call}\n}}\n")
}

fn buffer_prog(call: &str) -> String {
    format!("fn main() {{\n    var b bytes.buffer\n    {call}\n}}\n")
}

fn assert_e1002(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "must reject:\n{src}\n{:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for:\n{src}\ngot {:?}",
        r.diagnostics
    );
}

#[test]
fn builder_wrong_arity_rejects() {
    for call in [
        "b.write_string(\"a\", \"b\")",
        "b.write_string()",
        "b.write_byte()",
        "b.write_rune('a', 'b')",
        "b.write()",
        "b.grow()",
        "b.reset(1)",
        "fmt.println(b.string(1))",
        "fmt.println(b.len(3))",
        "n := b.cap(1)\n    fmt.println(n)",
    ] {
        assert_e1002(&builder_prog(call));
    }
}

#[test]
fn buffer_wrong_arity_rejects() {
    for call in [
        "b.write_string()",
        "b.write_string(\"a\", \"b\")",
        "b.truncate()",
        "b.next()",
        "b.grow(1, 2)",
        "b.reset(0)",
        "fmt.println(b.len(3))",
        "fmt.println(b.bytes(1))",
        "s, _ := b.read_string()\n    fmt.println(s)",
        "x, _ := b.read_bytes(10, 10)\n    fmt.println(len(x))",
        "c, _ := b.read_byte(1)\n    fmt.println(c)",
        "b.unread_byte(1)",
    ] {
        assert_e1002(&buffer_prog(call));
    }
}

#[test]
fn spread_on_non_variadic_method_rejects() {
    assert_e1002(&builder_prog(
        "xs := []string{\"a\"}\n    b.write_string(xs...)",
    ));
    assert_e1002(&buffer_prog("xs := []int{1}\n    b.truncate(xs...)"));
}

#[test]
fn wrong_arity_through_pointer_param_rejects() {
    assert_e1002(
        "fn fill(b *bytes.buffer) { b.write_string(\"a\", \"b\") }\n\
         fn main() {\n    var b bytes.buffer\n    fill(&b)\n}\n",
    );
    assert_e1002(
        "fn fill(b *strings.builder) { b.grow() }\n\
         fn main() {\n    var b strings.builder\n    fill(&b)\n}\n",
    );
}

#[test]
fn correct_arity_is_accepted() {
    let src = "fn fill(b *bytes.buffer) {\n\
                   b.write_string(\"xy\\n\")\n\
                   b.write_byte(98)\n\
                   b.write_rune('c')\n\
                   b.write([]byte(\"d\\n\"))\n\
                   b.grow(8)\n\
               }\n\
               fn main() {\n\
                   var sb strings.builder\n\
                   sb.grow(16)\n\
                   sb.write_string(\"ab\")\n\
                   sb.write_byte(99)\n\
                   sb.write_rune('d')\n\
                   sb.write([]byte(\"e\"))\n\
                   fmt.println(sb.string(), sb.len(), sb.cap() >= sb.len())\n\
                   sb.reset()\n\
                   var buf bytes.buffer\n\
                   fill(&buf)\n\
                   fmt.println(buf.len(), buf.cap() >= buf.len())\n\
                   line, _ := buf.read_string(10)\n\
                   fmt.println(line)\n\
                   bs, _ := buf.read_bytes(10)\n\
                   fmt.println(len(bs))\n\
                   c, _ := buf.read_byte()\n\
                   fmt.println(c)\n\
                   fmt.println(len(buf.next(1)), len(buf.bytes()), buf.string())\n\
                   buf.truncate(0)\n\
                   buf.reset()\n\
               }\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}
