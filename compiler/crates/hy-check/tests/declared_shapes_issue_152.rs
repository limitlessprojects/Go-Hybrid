//! Integration tests for Issue #152: expressions checked against declared shapes
//! (call arity HYB-E5002, return arity HYB-E5003, struct literal field names/dups
//! HYB-E5004, struct literal field values HYB-E5005, undefined struct field access
//! HYB-E5006, and map key / index type mismatches HYB-E5007).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let f = parse_file("test.hy", src).expect("test program must parse");
    check_file(&f)
}

#[test]
fn call_arity_too_few_and_too_many_reject_e5002() {
    let r_few = check(
        "fn tambah(a int, b int) int {\n\
           return a + b\n\
         }\n\
         fn main() {\n\
           fmt.println(\"{tambah(1)}\")\n\
         }",
    );
    assert_eq!(r_few.verdict, Verdict::Reject, "{:?}", r_few.diagnostics);
    assert!(r_few.diagnostics.iter().any(|d| d.code == "HYB-E5002"));

    let r_many = check(
        "fn satu(a int) int {\n\
           return a\n\
         }\n\
         fn main() {\n\
           fmt.println(\"{satu(1, 2)}\")\n\
         }",
    );
    assert_eq!(r_many.verdict, Verdict::Reject, "{:?}", r_many.diagnostics);
    assert!(r_many.diagnostics.iter().any(|d| d.code == "HYB-E5002"));
}

#[test]
fn task_and_method_call_arity_mismatch_reject_e5002() {
    let r_task = check(
        "fn worker(a int, b int) {\n\
           fmt.println(a + b)\n\
         }\n\
         fn main() {\n\
           task worker(1)\n\
         }",
    );
    assert_eq!(r_task.verdict, Verdict::Reject, "{:?}", r_task.diagnostics);
    assert!(r_task.diagnostics.iter().any(|d| d.code == "HYB-E5002"));

    let r_method = check(
        "struct counter {\n\
           n int\n\
         }\n\
         fn (c counter) add(delta int) int {\n\
           return c.n + delta\n\
         }\n\
         fn main() {\n\
           c := counter{n: 1}\n\
           fmt.println(c.add(1, 2))\n\
         }",
    );
    assert_eq!(r_method.verdict, Verdict::Reject, "{:?}", r_method.diagnostics);
    assert!(r_method.diagnostics.iter().any(|d| d.code == "HYB-E5002"));
}

#[test]
fn return_arity_mismatch_rejects_e5003_and_named_bare_return_accepts() {
    let r_few = check(
        "fn dua() (int, int) {\n\
           return 1\n\
         }\n\
         fn main() {\n\
           a, b := dua()\n\
           fmt.println(\"{a}{b}\")\n\
         }",
    );
    assert_eq!(r_few.verdict, Verdict::Reject, "{:?}", r_few.diagnostics);
    assert!(r_few.diagnostics.iter().any(|d| d.code == "HYB-E5003"));

    let r_many = check(
        "fn satu() int {\n\
           return 1, 2\n\
         }\n\
         fn main() {\n\
           x := satu()\n\
           fmt.println(x)\n\
         }",
    );
    assert_eq!(r_many.verdict, Verdict::Reject, "{:?}", r_many.diagnostics);
    assert!(r_many.diagnostics.iter().any(|d| d.code == "HYB-E5003"));

    let r_named = check(
        "fn named_ok() (code int) {\n\
           code = 7\n\
           return\n\
         }\n\
         fn main() {\n\
           fmt.println(named_ok())\n\
         }",
    );
    assert_eq!(r_named.verdict, Verdict::Accept, "{:?}", r_named.diagnostics);
}

#[test]
fn struct_literal_unknown_and_duplicate_fields_reject_e5004() {
    let r_unk = check(
        "struct a {\n\
           x int\n\
         }\n\
         fn main() {\n\
           v := a{y: 1}\n\
           fmt.println(v.x)\n\
         }",
    );
    assert_eq!(r_unk.verdict, Verdict::Reject, "{:?}", r_unk.diagnostics);
    assert!(r_unk.diagnostics.iter().any(|d| d.code == "HYB-E5004"));

    let r_dup = check(
        "struct a {\n\
           x int\n\
         }\n\
         fn main() {\n\
           v := a{x: 1, x: 2}\n\
           fmt.println(v.x)\n\
         }",
    );
    assert_eq!(r_dup.verdict, Verdict::Reject, "{:?}", r_dup.diagnostics);
    assert!(r_dup.diagnostics.iter().any(|d| d.code == "HYB-E5004"));
}

#[test]
fn struct_literal_field_value_type_mismatch_rejects_e5005_and_int_to_float_accepts() {
    let r_bad = check(
        "struct a {\n\
           x int\n\
         }\n\
         fn main() {\n\
           v := a{x: \"halo\"}\n\
           fmt.println(\"{v.x}\")\n\
         }",
    );
    assert_eq!(r_bad.verdict, Verdict::Reject, "{:?}", r_bad.diagnostics);
    assert!(r_bad.diagnostics.iter().any(|d| d.code == "HYB-E5005"));

    let r_var = check(
        "struct a {\n\
           x int\n\
         }\n\
         fn main() {\n\
           s := \"halo\"\n\
           v := a{x: s}\n\
           fmt.println(\"{v.x}\")\n\
         }",
    );
    assert_eq!(r_var.verdict, Verdict::Reject, "{:?}", r_var.diagnostics);
    assert!(r_var.diagnostics.iter().any(|d| d.code == "HYB-E5005"));

    let r_ok = check(
        "struct a {\n\
           f float64\n\
         }\n\
         fn main() {\n\
           v := a{f: 2}\n\
           fmt.println(\"{v.f}\")\n\
         }",
    );
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
}

#[test]
fn undefined_struct_field_access_and_assign_reject_e5006() {
    let r_read = check(
        "struct a {\n\
           x int\n\
         }\n\
         fn main() {\n\
           v := a{x: 1}\n\
           fmt.println(\"{v.y}\")\n\
         }",
    );
    assert_eq!(r_read.verdict, Verdict::Reject, "{:?}", r_read.diagnostics);
    assert!(r_read.diagnostics.iter().any(|d| d.code == "HYB-E5006"));

    let r_write = check(
        "struct a {\n\
           x int\n\
         }\n\
         fn main() {\n\
           v := a{x: 1}\n\
           v.y = 2\n\
           fmt.println(v.x)\n\
         }",
    );
    assert_eq!(r_write.verdict, Verdict::Reject, "{:?}", r_write.diagnostics);
    assert!(r_write.diagnostics.iter().any(|d| d.code == "HYB-E5006"));
}

#[test]
fn map_key_type_mismatch_and_float_slice_index_reject_e5007() {
    let r_map = check(
        "fn main() {\n\
           m := map[string]int{}\n\
           m[1] = 2\n\
           fmt.println(len(m))\n\
         }",
    );
    assert_eq!(r_map.verdict, Verdict::Reject, "{:?}", r_map.diagnostics);
    assert!(r_map.diagnostics.iter().any(|d| d.code == "HYB-E5007"));

    let r_float = check(
        "fn main() {\n\
           s := []int{1, 2}\n\
           fmt.println(\"{s[1.5]}\")\n\
         }",
    );
    assert_eq!(r_float.verdict, Verdict::Reject, "{:?}", r_float.diagnostics);
    assert!(r_float.diagnostics.iter().any(|d| d.code == "HYB-E5007"));
}
