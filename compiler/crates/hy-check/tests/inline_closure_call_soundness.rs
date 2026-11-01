//! #1877 — Arity and argument type checks for inline closure calls and task spawns.
//!
//! Under Row 13 of GUARANTEES.md (BUILDABLE), any program accepted by `hyb check`
//! must emit buildable Go.
//!
//! Previously, immediately-invoked function literals (`(fn(...) {})(...)`) and
//! task spawns (`task (fn(...) {})(...)`) fell through the checker without validating
//! argument count or parameter type compatibility, leading to compile errors in
//! `go build` (`too many arguments in call to func literal`, `not enough arguments...`,
//! `cannot use ... as ... value in argument to func literal`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert_eq!(cs, vec![want.to_string()], "for:\n{src}");
}

fn assert_has_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected code {want} in {cs:?}\nfor:\n{src}"
    );
}

fn assert_clean(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected clean, got: {cs:?}\nfor:\n{src}");
}

#[test]
fn test_inline_closure_call_too_many_args() {
    let src = r#"
        fn main() {
            _ = (fn(x int) int {
                return x + 1
            })(10, 20)
        }
    "#;
    assert_code(src, "HYB-E1002");
}

#[test]
fn test_inline_closure_call_too_few_args() {
    let src = r#"
        fn main() {
            _ = (fn(x int, y int) int {
                return x + y
            })(10)
        }
    "#;
    assert_code(src, "HYB-E1002");
}

#[test]
fn test_inline_closure_call_arg_type_mismatch() {
    let src = r#"
        fn main() {
            _ = (fn(x int) int {
                return x * 2
            })("invalid_string")
        }
    "#;
    assert_code(src, "HYB-E1002");
}

#[test]
fn test_task_inline_closure_call_too_many_args() {
    let src = r#"
        fn main() {
            managed("test", "actor") {
                task (fn(x int) {
                    _ = x
                })(1, 2)
            }
        }
    "#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_task_inline_closure_call_arg_type_mismatch() {
    let src = r#"
        fn main() {
            managed("test", "actor") {
                task (fn(x int) {
                    _ = x
                })("invalid_string")
            }
        }
    "#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn test_inline_closure_call_valid_accept() {
    let src = r#"
        fn main() {
            let a = (fn(x int) int {
                return x + 1
            })(10)
            _ = a
        }
    "#;
    assert_clean(src);
}

#[test]
fn test_closure_multi_value_arity() {
    let src_clean = r#"
        fn two() (int, int) {
            return 1, 2
        }
        fn main() {
            _ = (fn(a int, b int) int {
                return a + b
            })(two())
        }
    "#;
    assert_clean(src_clean);

    let src_too_many = r#"
        fn two() (int, int) {
            return 1, 2
        }
        fn main() {
            _ = (fn(a int) int {
                return a
            })(two())
        }
    "#;
    assert_code(src_too_many, "HYB-E1002");
}
