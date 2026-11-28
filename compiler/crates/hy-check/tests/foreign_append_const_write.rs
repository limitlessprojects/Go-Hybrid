//! foreign_append_const_write.rs — T22 (HYB-E3006) on the package-level
//! append family (`fmt.append`/`appendf`/`appendln`, `strconv.append_*`).
//! Like the builtin `append(dst, ..)` (append_const_write.rs) they write into
//! `dst`'s spare capacity in place, so a bounded reslice of a `const` (or a
//! local alias of one) as the destination overwrites the `const`'s elements
//! and races with any other task doing the same (`go run -race` reports the
//! write in `runtime.slicecopy` under `fmt.Append`). Appending to the
//! `const` itself or an open-ended tail always reallocates (`cap == len`)
//! and stays legal, exactly as for the builtin.

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

fn assert_e3006(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "{}: expected HYB-E3006, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

fn assert_accept(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{}: got {:?}", what, codes(&r));
}

const RACY_TASKS: &str = "const table = []byte{48, 48, 48, 48, 48, 48, 48, 48}

fn work(n int, done chan<- bool) {
    for i in 0..20000 {
        out := fmt.append(table[:0], n)
        _ = out
    }
    done <- true
}

fn main() {
    done := make(chan bool, 2)
    task work(1111, done)
    task work(2222, done)
    <-done
    <-done
    s := string(table)
    fmt.println(\"{s}\")
}
";

#[test]
fn fmt_append_into_const_reslice_across_tasks_is_e3006() {
    assert_e3006(RACY_TASKS, "two tasks fmt.append into table[:0]");
}

#[test]
fn fmt_appendf_and_appendln_into_const_reslice_are_e3006() {
    assert_e3006(
        "const table = []byte{48, 48, 48}\n\nfn main() {\n    a := fmt.appendf(table[:1], \"%d\", 7)\n    _ = a\n}\n",
        "fmt.appendf(table[:1], ..)",
    );
    assert_e3006(
        "const table = []byte{48, 48, 48}\n\nfn main() {\n    a := fmt.appendln(table[0:0], 7)\n    _ = a\n}\n",
        "fmt.appendln(table[0:0], ..)",
    );
}

#[test]
fn strconv_append_into_const_reslice_is_e3006() {
    assert_e3006(
        "const table = []byte{48, 48, 48, 48, 48}\n\nfn main() {\n    a := strconv.append_bool(table[:0], true)\n    _ = a\n}\n",
        "strconv.append_bool(table[:0], ..)",
    );
    assert_e3006(
        "const table = []byte{48, 48, 48, 48, 48}\n\nfn main() {\n    a := strconv.append_quote(table[1:2], \"x\")\n    _ = a\n}\n",
        "strconv.append_quote(table[1:2], ..)",
    );
}

#[test]
fn append_into_local_alias_of_const_reslice_is_e3006() {
    assert_e3006(
        "const table = []byte{48, 48, 48, 48}\n\nfn main() {\n    head := table[1:3]\n    a := fmt.append(head, 1)\n    _ = a\n}\n",
        "local alias of a bounded const reslice",
    );
}

#[test]
fn append_into_fn_local_const_reslice_is_e3006() {
    assert_e3006(
        "fn main() {\n    const table = []byte{48, 48, 48}\n    _ = strconv.append_bool(table[:0], false)\n}\n",
        "fn-local const, discarded result",
    );
}

#[test]
fn append_onto_whole_const_or_open_tail_is_accepted() {
    assert_accept(
        "const table = []byte{48, 48}\n\nfn main() {\n    a := fmt.append(table, 1)\n    b := strconv.append_bool(table[1:], true)\n    c := append(table, 49)\n    s := string(a) + string(b) + string(c)\n    fmt.println(\"{s}\")\n}\n",
        "whole const / open-ended tail reallocate (cap == len)",
    );
}

#[test]
fn const_as_source_operand_and_fresh_local_dst_are_accepted() {
    assert_accept(
        "const table = []byte{48, 48, 48, 48}

fn work(n int, done chan<- bool) {
    buf := []byte{48, 48, 48, 48}
    for i in 0..100 {
        out := fmt.append(buf[:0], n)
        _ = out
        o2 := strconv.append_bool(buf[:0], true)
        _ = o2
    }
    done <- true
}

fn main() {
    done := make(chan bool, 2)
    task work(1111, done)
    task work(2222, done)
    <-done
    <-done
    t := fmt.append([]byte{}, table[0])
    s := string(t)
    fmt.println(\"{s}\")
}
",
        "per-task local buffer as dst; const only read",
    );
}
