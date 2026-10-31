//! fnlit_call.rs — regression tests for #1877: an immediately-invoked
//! function literal is checked for arity and argument types.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn too_many_args_to_fnlit_is_e1002() {
    let r = check(
        "fn main() {\n    v := (fn(x int) int {\n        return x + 1\n    })(10, 20)\n    _ = v\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for too many closure args, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn too_few_args_to_fnlit_is_e1002() {
    let r = check("fn main() {\n    v := (fn(x int) int {\n        return x\n    })()\n    _ = v\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for too few closure args, got {cs:?}"
    );
}

#[test]
fn wrong_arg_type_to_fnlit_is_e1002() {
    let r = check(
        "fn main() {\n    v := (fn(x int) int {\n        return x\n    })(\"nope\")\n    _ = v\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a wrong-typed closure arg, got {cs:?}"
    );
}

#[test]
fn wrong_arity_in_task_spawn_is_e1002() {
    let r = check("fn main() {\n    task (fn(x int) {\n        _ = x\n    })(1, 2)\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a task closure arity mismatch, got {cs:?}"
    );
}

#[test]
fn well_formed_fnlit_call_accepts() {
    let r = check(
        "fn main() {\n    v := (fn(x int) int {\n        return x + 1\n    })(41)\n    _ = v\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "a well-formed closure call must stay accepted, got {cs:?}"
    );
}
