use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {:?}", d));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.clone()).collect()
}

fn count_code(r: &hy_check::CheckResult, code: &str) -> usize {
    error_codes(r).iter().filter(|c| c.as_str() == code).count()
}

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(cs.iter().any(|c| c == code), "expected {}, got {:?}", code, cs);
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    let errs = error_codes(&r);
    assert!(errs.is_empty(), "expected clean accept, got {:?}\nsrc: {}", errs, src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn const_with_interface_or_option_ascription_is_rejected_with_e6003() {
    assert_has(&check("package main\nconst x any = 42\nfn main() { _ = x }\n"), "HYB-E6003");
    assert_has(&check("package main\nconst s any = \"hello\"\nfn main() { _ = s }\n"), "HYB-E6003");
    assert_has(&check("package main\ntype iface = any\nconst c iface = true\nfn main() { _ = c }\n"), "HYB-E6003");
    let r_local = check("package main\nfn main() {\n    const x any = 42\n    const s any = \"hello\"\n    _ = x\n    _ = s\n}\n");
    assert_eq!(count_code(&r_local, "HYB-E6003"), 2, "{:?}", r_local.diagnostics);
    let r_closure = check("package main\nfn main() {\n    f := fn() {\n        const x any = 42\n        const s any = \"hello\"\n        _ = x\n        _ = s\n    }\n    f()\n}\n");
    assert_eq!(count_code(&r_closure, "HYB-E6003"), 2, "{:?}", r_closure.diagnostics);
    let r_opt = check("package main\nconst s option[string] = \"hello\"\nconst n = len(s)\nfn main() { _ = n }\n");
    assert_eq!(count_code(&r_opt, "HYB-E6003"), 2, "{:?}", r_opt.diagnostics);
}

#[test]
fn local_const_with_non_const_ascription_or_init_in_downstream_const_is_rejected_with_e6003() {
    let r_local = check("package main\nfn main() {\n    const s any = \"hello\"\n    const n = len(s)\n    const x any = 42\n    const y = x + 1\n    _ = n\n    _ = y\n}\n");
    assert_eq!(count_code(&r_local, "HYB-E6003"), 4, "{:?}", r_local.diagnostics);
    let r_closure = check("package main\nfn main() {\n    f := fn() {\n        const s any = \"hello\"\n        const n = len(s)\n        const x any = 42\n        const y = x + 1\n        _ = n\n        _ = y\n    }\n    f()\n}\n");
    assert_eq!(count_code(&r_closure, "HYB-E6003"), 4, "{:?}", r_closure.diagnostics);
    let r_shadow_str = check("package main\ntype string = int\nfn main() {\n    const s = string(65)\n    const n = len(s)\n    _ = n\n}\n");
    assert_eq!(count_code(&r_shadow_str, "HYB-E6003"), 1, "{:?}", r_shadow_str.diagnostics);
    let r_non_const = check("package main\nfn get_str() string { return \"hi\" }\nfn get_int() int { return 7 }\nfn main() {\n    const s = get_str()\n    const n = len(s)\n    const x = get_int()\n    const y = x + 1\n    _ = n\n    _ = y\n}\n");
    assert_eq!(count_code(&r_non_const, "HYB-E6003"), 4, "{:?}", r_non_const.diagnostics);
}

#[test]
fn const_conversion_and_len_shadowed_by_enum_const_import_or_cyclic_alias_is_rejected_with_e6003() {
    assert_has(&check("package main\nenum msg { int8(int) }\nconst c = int8(42)\nfn main() { _ = c }\n"), "HYB-E6003");
    assert_has(&check("package main\nenum msg { len(string) }\nconst n = len(\"hello\")\nfn main() { _ = n }\n"), "HYB-E6003");
    assert_has(&check("package main\nenum msg { string(int) }\nconst n = len(string(65))\nfn main() { _ = n }\n"), "HYB-E6003");
    let r_enum_ty = check("package main\nenum int8 { a }\ntype my_int = int8\nconst c1 = int8(42)\nconst c2 = my_int(42)\nfn main() { _ = c1\n _ = c2 }\n");
    assert_eq!(count_code(&r_enum_ty, "HYB-E6003"), 2, "{:?}", r_enum_ty.diagnostics);
    assert_has(&check("package main\nenum string { a }\ntype my_str = string\nconst n = len(my_str(\"hello\"))\nfn main() { _ = n }\n"), "HYB-E6003");
    let r_const_shadow = check("package main\nconst int8 = [1, 2]\nconst len = [1, 2]\nconst c = int8(0)\nconst n = len(\"hello\")\nfn main() { _ = int8\n _ = len\n _ = c\n _ = n }\n");
    assert_eq!(count_code(&r_const_shadow, "HYB-E6003"), 2, "{:?}", r_const_shadow.diagnostics);
    let r_cyclic = check("package main\ntype int8 = my_int\ntype my_int = int8\ntype string = my_str\ntype my_str = string\nconst c = my_int(42)\nconst n = len(my_str(\"hello\"))\nfn main() { _ = c\n _ = n }\n");
    assert_eq!(count_code(&r_cyclic, "HYB-E6003"), 2, "{:?}", r_cyclic.diagnostics);
    let r_named = check("package main\nconst c = int8(x = 42)\nconst n = len(s = \"hello\")\nfn main() { _ = c\n _ = n }\n");
    assert_eq!(count_code(&r_named, "HYB-E6003"), 2, "{:?}", r_named.diagnostics);
}

#[test]
fn local_const_chain_with_spec_conversions_and_type_aliases_is_accepted() {
    assert_accepts("package main\ntype my_str = string\ntype my_char = char\ntype my_int = i32\nfn main() {\n    const a = f64(42)\n    const a2 = a + 1.0\n    const b = u8(7)\n    const b2 = b + 1\n    const c = my_char(65)\n    const c2 = c\n    const d = my_int(10)\n    const d2 = d + 5\n    const s = my_str(\"hello\")\n    const n = len(s)\n    int8 := 99\n    const after_shadow = d2 + 1\n    f := fn() {\n        const cs = my_str(\"world\")\n        const cn = len(cs)\n        _ = cn\n    }\n    f()\n    _ = a2\n    _ = b2\n    _ = c2\n    _ = d2\n    _ = n\n    _ = int8\n    _ = after_shadow\n}\n");
}
