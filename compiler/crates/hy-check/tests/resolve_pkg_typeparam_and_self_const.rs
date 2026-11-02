//! Regression tests for `HYB-E1001` (`resolve.rs`):
//! 1. Bare builtin and imported package names (`fmt`, `strings`, `os`, `http`,
//!    `serde_json`, etc.) in value, call, or assignment-place positions
//!    (`x := fmt`, `ch <- strings`, `fmt()`, `os = 1`, `fmt[int](1)`) must be
//!    rejected with `HYB-E1001`, while package selector bases (`fmt.println`,
//!    `os.stdout.write`, `serde_json.parse`) and shadowed local/param/var/const/fn
//!    declarations (`os := 1; os = 2`) remain accepted.
//! 2. Generic type parameters (`t` on `fn id[t](x t)`, receiver `fn (b box[t]) m()`,
//!    or `struct box[t]`) must not be accepted as bare runtime values (`y := t`,
//!    `return t`, `_ = t`, `m[t]`) or array lengths (`[t]int`), rejecting with
//!    `HYB-E1001`, while remaining accepted in valid type/instantiation positions
//!    (`new(t)`, `make([]t, n)`, `id[t](x)`, `box[t]{...}`, `var z t`).
//! 3. Self-referential file-level `const` initializers (`const xs = [xs[0]]`,
//!    `const x = x`, `const a [a]int = ...`) and forward-referenced composite
//!    `const`s (`const a = [b[0]]; const b = [a[0]]`) must be rejected with
//!    `HYB-E1001` rather than resolving against their own declaration.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
}

fn assert_no_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1001"),
        "must NOT fire HYB-E1001 for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn bare_package_names_in_value_call_and_place_positions_reject_e1001() {
    assert_rejects_e1001("fn main() {\n    x := fmt\n    _ = x\n}\n");
    assert_rejects_e1001(
        "import \"strings\"\nfn main() {\n    ch := make(chan int, 1)\n    ch <- strings\n}\n",
    );
    assert_rejects_e1001("fn main() {\n    fmt()\n}\n");
    assert_rejects_e1001("fn main() {\n    os = 1\n}\n");
    assert_rejects_e1001("fn main() {\n    _ = fmt[int](1)\n}\n");
    assert_rejects_e1001(
        "import \"rust/serde_json\"\nfn main() {\n    x := serde_json\n    _ = x\n}\n",
    );
}

#[test]
fn package_selectors_and_shadowed_package_names_do_not_fire_e1001() {
    assert_no_e1001("fn main() {\n    fmt.println(\"ok\")\n}\n");
    assert_no_e1001(
        "import \"strings\"\nfn main() {\n    s := strings.repeat(\"a\", 2)\n    fmt.println(s)\n}\n",
    );
    assert_no_e1001(
        "import \"rust/serde_json\"\nfn main() {\n    doc := serde_json.parse(\"[]\")?\n    _ = doc\n}\n",
    );
    assert_no_e1001("fn main() {\n    os := 1\n    os = 2\n    fmt.println(os)\n}\n");
    assert_no_e1001("fn take(fmt int) int {\n    return fmt + 1\n}\nfn main() {\n    _ = take(1)\n}\n");
}

#[test]
fn generic_type_params_in_value_and_array_len_positions_reject_e1001() {
    assert_rejects_e1001("fn bad[t any](x t) t {\n    y := t\n    _ = y\n    return x\n}\nfn main() {}\n");
    assert_rejects_e1001("fn bad_ret[t any](x t) t {\n    _ = x\n    return t\n}\nfn main() {}\n");
    assert_rejects_e1001("fn bad_discard[t any](x t) {\n    _ = x\n    _ = t\n}\nfn main() {}\n");
    assert_rejects_e1001(
        "struct box[t any] {\n    v t\n}\nfn (b box[t]) bad() int {\n    _ = t\n    return 0\n}\nfn main() {}\n",
    );
    assert_rejects_e1001(
        "struct bad_arr[t any] {\n    a [t]int\n}\nfn main() {}\n",
    );
    assert_rejects_e1001(
        "fn bad_sig[t any](a [t]int) {\n    _ = a\n}\nfn main() {}\n",
    );
    assert_rejects_e1001(
        "fn bad_local_arr[t any](x t) {\n    var a [t]int\n    _ = a\n    _ = x\n}\nfn main() {}\n",
    );
}

#[test]
fn generic_type_params_in_type_and_instantiation_positions_do_not_fire_e1001() {
    assert_no_e1001(
        "struct box[t any] {\n    v t\n}\nfn id[t any](x t) t {\n    return x\n}\nfn (b *box[t]) forward(x t) *t {\n    var z t = id[t](x)\n    s := make([]t, 1)\n    s[0] = z\n    nb := box[t]{v: s[0]}\n    _ = nb\n    return new(t)\n}\nfn main() {}\n",
    );
}

#[test]
fn self_referential_and_cyclic_composite_file_consts_reject_e1001() {
    assert_rejects_e1001("const xs = [xs[0]]\nfn main() {\n    _ = xs\n}\n");
    assert_rejects_e1001("const m = map[string]int{\"k\": m[\"k\"]}\nfn main() {\n    _ = m\n}\n");
    assert_rejects_e1001(
        "struct box {\n    v int\n}\nconst b = box{v: b.v}\nfn main() {\n    _ = b\n}\n",
    );
    assert_rejects_e1001("const x = x\nfn main() {\n    _ = x\n}\n");
    assert_rejects_e1001("const a [a]int = [1]int{1}\nfn main() {\n    _ = a\n}\n");
    assert_rejects_e1001(
        "const a = [b[0]]\nconst b = [a[0]]\nfn main() {\n    _ = a\n    _ = b\n}\n",
    );
}

#[test]
fn non_cyclic_and_forward_scalar_file_consts_do_not_fire_e1001() {
    assert_no_e1001("const a = 1\nconst b = a + 2\nconst xs = [b]\nfn main() {\n    _ = xs\n}\n");
    assert_no_e1001("const a int8 = b\nconst b = 10\nfn main() {\n    _ = a\n}\n");
    assert_no_e1001("const xs = [1, 2]\nconst ys = [xs[0]]\nfn main() {\n    _ = ys\n}\n");
}
