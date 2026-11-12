//! option_payload_width.rs — `o.unwrap()` / `o.or(d)` on an `option[T]` yield
//! a value of Go type `T`. With `T` a predeclared numeric type the #2712 width
//! check treated the result as width-less, so `var z int64 = op.unwrap()` with
//! `op option[i8]` passed `hyb check` and failed `go build`.
//! Issue #3175 — GUARANTEES row 13 (BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.severity == hy_diag::Severity::Error && d.code == "HYB-E1002")
}

#[test]
fn option_payload_width_mismatch_rejects_e1002() {
    let cases = [
        "fn main() {\n    var op option[i8] = some(i8(1))\n    var z int64 = op.unwrap()\n    fmt.println(z)\n}\n",
        "fn get() option[u8] { return some(u8(2)) }\nfn main() {\n    var z int = get().unwrap()\n    fmt.println(z)\n}\n",
        "fn take(n int) { fmt.println(n) }\nfn main() {\n    var op option[i64] = some(i64(3))\n    take(op.unwrap())\n}\n",
        "fn f(op option[i32]) int { return op.unwrap() }\nfn main() { fmt.println(f(some(i32(1)))) }\n",
        "fn main() {\n    var op option[i8] = some(i8(1))\n    v := op.unwrap()\n    var z int = v\n    fmt.println(z)\n}\n",
        "fn main() {\n    var op option[f32] = some(f32(1.5))\n    var z float64 = op.unwrap()\n    fmt.println(z)\n}\n",
        "fn main() {\n    var op option[i8] = some(i8(1))\n    var z int = 0\n    z = op.unwrap() + z\n    fmt.println(z)\n}\n",
        "fn f(op option[i32]) {\n    var z int = op.unwrap()\n    fmt.println(z)\n}\nfn main() { f(some(i32(1))) }\n",
        "fn f() int {\n    var op option[i32] = some(i32(1))\n    return op.unwrap()\n}\nfn main() { fmt.println(f()) }\n",
        "fn main() {\n    var op option[i8] = none\n    var z int = op.or(i8(4))\n    fmt.println(z)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn option_payload_matching_width_still_accepts() {
    let cases = [
        "fn main() {\n    var op option[i8] = some(i8(1))\n    var z i8 = op.unwrap()\n    var y int = int(op.or(i8(2)))\n    w := op.unwrap() + 1\n    fmt.println(z, y, w)\n}\n",
        "fn main() {\n    op := some(1)\n    var z int = op.unwrap()\n    fmt.println(z)\n}\n",
        "fn get() option[u8] { return some(u8(2)) }\nfn main() {\n    var z u8 = get().unwrap()\n    var w int = int(get().or(u8(1)))\n    fmt.println(z, w)\n}\n",
        "fn f() i32 {\n    var op option[i32] = some(i32(1))\n    return op.unwrap()\n}\nfn main() { fmt.println(f()) }\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected acceptance for:\n{}\n{:?}", src, r.diagnostics);
    }
}
