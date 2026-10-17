//! else_if_cond_closure.rs — regression tests: closures sited in an
//! `else if` CONDITION inside a closure body must be collected for the
//! nested-closure boundary pass (the #229 / PR #230 walk). Before the fix the
//! `Stmt::If` arm of `check_stmt_boundaries` skipped collection for an
//! If-shaped `otherwise`, so every closure in an else-if chain condition at
//! closure depth >= 2 (and its whole subtree) got no boundary check.
//! Each program is placement-only: the same payload in a then-body or plain
//! `else { }` block is already rejected with the same code.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast =
        parse_file("inline.hy", src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

/// nested closure in an else-if condition spawns a task with the outer closure's unmoved map param (raced at runtime before the fix).
#[test]
fn else_if_cond_closure_task_of_outer_param_is_rejected_e3001() {
    let src = r#"fn worker(m map[string]int, done chan<- bool) {
	m["k"] = 1
	done <- true
}

fn main() {
	done := make(chan bool, 1)
	m := 1
	fmt.println(m)

	outer := fn(m map[string]int) {
		if 1 == 2 {
			fmt.println("then")
		} else if (fn() bool {
			task worker(m, done)
			return false
		})() {
			fmt.println("cond took the closure's value")
		} else {
			fmt.println("else")
		}
	}

	shared := map[string]int{"k": 0}
	outer(shared)
	shared["k"] = 9
	<-done
	fmt.println(shared["k"])
}
"#;
    assert_rejects_with(src, "HYB-E3001");
}

/// else-if-condition twin of nested_closure_env.rs test 3: the cond closure writes a map the outer closure already moved into a task.
#[test]
fn else_if_cond_closure_use_after_outer_move_is_rejected_e3003() {
    let src = r#"fn worker(m map[string]int, done chan<- bool) {
	m["k"] = 1
	done <- true
}

fn main() {
	done := make(chan bool, 1)
	m := 1
	fmt.println(m)

	outer := fn() {
		m := map[string]int{"k": 0}
		task worker(move m, done)
		if 1 == 2 {
			fmt.println("then")
		} else if (fn() bool {
			m["k"] = 2
			return false
		})() {
			fmt.println("cond")
		}
	}

	outer()
	<-done
}
"#;
    assert_rejects_with(src, "HYB-E3003");
}

/// else-if-condition twin of nested_closure_env.rs test 2: alias move of an outer-closure local.
#[test]
fn else_if_cond_closure_alias_move_is_rejected_e3004() {
    let src = r#"fn worker(m map[string]int, done chan<- bool) {
	m["k"] = 1
	done <- true
}

fn main() {
	done := make(chan bool, 2)
	m := 1
	fmt.println(m)

	outer := fn() {
		mm := map[string]int{"k": 0}
		if 1 == 2 {
			fmt.println("then")
		} else if (fn() bool {
			alias := mm
			task worker(move alias, done)
			return false
		})() {
			fmt.println("cond")
		}
	}

	outer()
	<-done
}
"#;
    assert_rejects_with(src, "HYB-E3004");
}

/// a depth-3 grandchild closure under an else-if condition closure must be reached too.
#[test]
fn else_if_cond_closure_grandchild_task_is_rejected_e3001() {
    let src = r#"fn worker(m map[string]int, done chan<- bool) {
	m["k"] = 1
	done <- true
}

fn main() {
	done := make(chan bool, 1)
	m := 1
	fmt.println(m)

	outer := fn(m map[string]int) {
		if 1 == 2 {
			fmt.println("then")
		} else if (fn() bool {
			grandchild := fn() {
				task worker(m, done)
			}
			grandchild()
			return false
		})() {
			fmt.println("cond")
		}
	}

	shared := map[string]int{"k": 0}
	outer(shared)
	<-done
	fmt.println(shared["k"])
}
"#;
    assert_rejects_with(src, "HYB-E3001");
}

/// channel-send boundary inside an else-if condition closure.
#[test]
fn else_if_cond_closure_send_of_outer_param_is_rejected_e3002() {
    let src = r#"fn main() {
	ch := make(chan map[string]int, 1)
	m := 1
	fmt.println(m)

	outer := fn(m map[string]int) {
		if 1 == 2 {
			fmt.println("then")
		} else if (fn() bool {
			ch <- m
			return false
		})() {
			fmt.println("cond")
		}
	}

	shared := map[string]int{"k": 0}
	outer(shared)
	shared["k"] = 9
	v := <-ch
	fmt.println(v["k"])
	fmt.println(shared["k"])
}
"#;
    assert_rejects_with(src, "HYB-E3002");
}

/// Positive control: an else-if condition closure with NO boundary statement
/// stays outside the checked subset (A5 / GUARANTEES row 5) — the fix must
/// not start rejecting capture-only closures.
#[test]
fn else_if_cond_closure_without_boundary_is_still_accepted() {
    let src = r#"fn main() {
	m := 1
	fmt.println(m)

	outer := fn(m map[string]int) {
		if 1 == 2 {
			fmt.println("then")
		} else if (fn() bool {
			return m["k"] == 0
		})() {
			fmt.println("cond")
		}
	}

	shared := map[string]int{"k": 0}
	outer(shared)
	fmt.println(shared["k"])
}
"#;
    let r = check(src);
    assert!(
        codes(&r).is_empty(),
        "expected no errors, got {:?}",
        codes(&r)
    );
}
