//! SPEC-LANGUAGE T16: `==`/`!=` are defined only for unit-only enums;
//! data-carrying enums compare via `match` (issue #2444). The HYB-E2006
//! equality check walked map/slice/fn/struct shapes only, so `a == b` on a
//! data-carrying enum passed `hyb check` with 0 diagnostics.

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn e2006(r: &CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E2006")
        .count()
}

const SHAPE: &str = "enum shape {\n    circle(int)\n    point\n}\n\n";

#[test]
fn issue_2444_repro_rejected() {
    let r = check(
        "enum packet {\n    data([]byte)\n    ping\n}\n\nfn is_same(a packet, b packet) bool {\n    return a == b\n}\n\nfn main() {\n    p1 := packet.data([]byte{1, 2})\n    p2 := packet.data([]byte{1, 2})\n    _ = is_same(p1, p2)\n}\n",
    );
    assert_eq!(e2006(&r), 1, "{:?}", r.diagnostics);
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E2006")
        .unwrap();
    assert!(d.message.contains("data-carrying enum"), "{}", d.message);
    assert!(d.message.contains("T16"), "{}", d.message);
}

#[test]
fn every_operand_form_of_a_data_carrying_enum_rejected() {
    for (what, body) in [
        (
            "constructor-bound locals",
            "    a := shape.circle(1)\n    b := shape.circle(1)\n    fmt.println(a == b)\n",
        ),
        (
            "!= against a unit variant",
            "    a := shape.circle(1)\n    fmt.println(a != shape.point)\n",
        ),
        (
            "typed var vs unit variant",
            "    var b shape = shape.point\n    fmt.println(shape.point == b)\n",
        ),
        (
            "constructor on both sides",
            "    fmt.println(shape.circle(1) == shape.circle(1))\n",
        ),
    ] {
        let r = check(&format!("{SHAPE}fn main() {{\n{body}}}\n"));
        assert_eq!(e2006(&r), 1, "{what}: {:?}", r.diagnostics);
    }
}

#[test]
fn unit_only_enum_equality_stays_legal() {
    let r = check(
        "enum mode {\n    read\n    write\n}\n\nfn main() {\n    m := mode.read\n    fmt.println(m == mode.write)\n    fmt.println(m != mode.read)\n}\n",
    );
    assert_eq!(e2006(&r), 0, "{:?}", r.diagnostics);
}

#[test]
fn match_on_a_data_carrying_enum_stays_legal() {
    let r = check(&format!(
        "{SHAPE}fn main() {{\n    a := shape.circle(2)\n    match a {{\n        circle(r): fmt.println(r == 2)\n        point:     fmt.println(false)\n    }}\n}}\n"
    ));
    assert_eq!(e2006(&r), 0, "{:?}", r.diagnostics);
}
