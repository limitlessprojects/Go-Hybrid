use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast =
        parse_file("inline.hy", src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e6003(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003, got {:?}\nsrc:\n{}",
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected clean accept, got {:?}\nsrc:\n{}",
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn const_int_or_bool_to_string_conversion_is_rejected_e6003() {
    assert_rejects_e6003(
        "const s = string(65)\n\
         fn main() { fmt.println(s) }\n",
    );
    assert_rejects_e6003(
        "fn main() {\n\
             const s = string(65)\n\
             fmt.println(s)\n\
         }\n",
    );
    assert_rejects_e6003(
        "type my_str = string\n\
         const s = my_str(65)\n\
         fn main() { fmt.println(s) }\n",
    );
    assert_rejects_e6003(
        "const s = string(true)\n\
         fn main() { fmt.println(s) }\n",
    );
    assert_rejects_e6003(
        "const n = len(string(65))\n\
         fn main() { fmt.println(n) }\n",
    );
    assert_rejects_e6003(
        "const n = len(string(true))\n\
         fn main() { fmt.println(n) }\n",
    );
}

#[test]
fn const_cross_family_scalar_conversions_are_rejected_e6003() {
    assert_rejects_e6003(
        "const b = bool(1)\n\
         fn main() { fmt.println(b) }\n",
    );
    assert_rejects_e6003(
        "const b = bool(\"true\")\n\
         fn main() { fmt.println(b) }\n",
    );
    assert_rejects_e6003(
        "const x = int(\"ok\")\n\
         fn main() { fmt.println(x) }\n",
    );
    assert_rejects_e6003(
        "const x = int(true)\n\
         fn main() { fmt.println(x) }\n",
    );
    assert_rejects_e6003(
        "const f = float64(\"3.14\")\n\
         fn main() { fmt.println(f) }\n",
    );
    assert_rejects_e6003(
        "const f = float64(false)\n\
         fn main() { fmt.println(f) }\n",
    );
}

#[test]
fn cyclic_string_const_len_terminates_and_rejects_e6003() {
    assert_rejects_e6003(
        "const a = b + b\n\
         const b = a + a\n\
         const n = len(a)\n\
         fn main() { fmt.println(\"{a} {b} {n}\") }\n",
    );
}

#[test]
fn valid_string_bool_and_numeric_const_conversions_still_accept() {
    assert_accepts(
        "type my_str = string\n\
         type my_bool = bool\n\
         const s1 = string(\"hello\")\n\
         const s2 = my_str(\"world\")\n\
         const n = len(s1 + s2)\n\
         const b1 = bool(true)\n\
         const b2 = my_bool(!b1)\n\
         const x = int64(42)\n\
         fn main() {\n\
             const local_flag = true\n\
             const inv = !local_flag\n\
             fmt.println(\"{s1} {s2} {n} {b1} {b2} {x} {inv}\")\n\
         }\n",
    );
}
