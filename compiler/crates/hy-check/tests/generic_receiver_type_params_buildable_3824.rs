use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file)
}

fn assert_rejected(src: &str, expected_code: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let errs: Vec<String> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect();
    assert!(
        errs.iter().any(|c| c == expected_code),
        "expected {expected_code} for {what}, got {errs:?}"
    );
}

fn assert_accepted(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for {what}, got Reject with {:#?}",
        r.diagnostics
    );
}

#[test]
fn generic_receiver_type_params_rejected_in_builtins_and_shadowing() {
    // 1. Builtins on unconstrained receiver type parameter (field, param, and inferred local) -> HYB-E2005
    assert_rejected(
        "struct box[t any] { val t }\nfn (b *box[t]) get_len() int { return len(b.val) }\nfn main() {}\n",
        "HYB-E2005",
        "len(b.val) on generic receiver field",
    );
    assert_rejected(
        "struct box[t any] { val t }\nfn (b box[t]) get_cap(x t) int {\n  _ = b\n  return cap(x)\n}\nfn main() {}\n",
        "HYB-E2005",
        "cap(x) on generic receiver method param",
    );
    assert_rejected(
        "struct box[t any] { val t }\nfn (b *box[t]) do_close() { close(b.val) }\nfn main() {}\n",
        "HYB-E2005",
        "close(b.val) on generic receiver field",
    );
    assert_rejected(
        "struct box[t any] { val t }\nfn (b *box[t]) do_delete() { delete(b.val, 1) }\nfn main() {}\n",
        "HYB-E2005",
        "delete(b.val, 1) on generic receiver field",
    );
    assert_rejected(
        "struct box[t any] { val t }\nfn (b *box[t]) do_clear() {\n  y := b.val\n  clear(y)\n}\nfn main() {}\n",
        "HYB-E2005",
        "clear(y) on local inferred from generic receiver field",
    );
    assert_rejected(
        "struct box[t any] { val t }\nfn (b *box[t]) do_copy(ys []int) int { return copy(b.val, ys) }\nfn main() {}\n",
        "HYB-E2005",
        "copy(b.val, ys) on generic receiver field",
    );
    assert_rejected(
        "struct box[t any] { val t }\nfn (b *box[t]) do_append() { _ = append(b.val, 1) }\nfn main() {}\n",
        "HYB-E2005",
        "append(b.val, 1) on generic receiver field",
    );
    assert_rejected(
        "struct box[t any] { val t }\nfn (b *box[t]) do_min(x t) t { return min(b.val, x) }\nfn main() {}\n",
        "HYB-E2005",
        "min(b.val, x) on generic receiver field",
    );

    // 2. Receiver type parameter shadowing lowering package (`os` / `fmt`) -> HYB-E1002
    assert_rejected(
        "import \"fmt\"\nstruct box[os any] { val os }\nfn (b *box[os]) log() {\n  _ = b\n  fmt.eprintln(\"err\")\n}\nfn main() {}\n",
        "HYB-E1002",
        "receiver type param `os` shadowing `os.Stderr` in fmt.eprintln",
    );

    // 3. Positive controls: container of receiver type parameter (`[]t`, `map[string]t`, `chan t`) stays accepted
    assert_accepted(
        "struct box[t any] { items []t }\nfn (b *box[t]) count(x t) int {\n  b.items = append(b.items, x)\n  return len(b.items)\n}\nfn main() {}\n",
        "slice of receiver type parameter []t in append and len",
    );
}