//! append_const_write.rs — T22 (HYB-E3006) on `append`: `append(dst, ..)`
//! writes into `dst`'s spare capacity in place, so a bounded reslice of a
//! `const` (`c[:k]`, `c[a:b]`, or a local alias of one) as the destination
//! overwrites the `const`'s elements — and races with any task reading it.
//! Appending to the `const` itself, a field/element of it, or an open-ended
//! tail `c[i:]` always reallocates (a `const` composite has `cap == len`)
//! and stays legal.

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

#[test]
fn append_into_const_reslice_is_e3006() {
    assert_e3006(
        "const primes = []int{2, 3, 5, 7}\n\nfn main() {\n    head := append(primes[:1], 99)\n    _ = head\n}\n",
        "append(primes[:1], ..) overwrites primes[1]",
    );
}

#[test]
fn append_into_fn_local_const_reslice_is_e3006() {
    assert_e3006(
        "fn main() {\n    const primes = []int{2, 3, 5, 7}\n    _ = append(primes[0:0], 11, 13)\n}\n",
        "fn-local const, discarded result",
    );
}

#[test]
fn append_into_alias_of_const_reslice_is_e3006() {
    assert_e3006(
        "const primes = []int{2, 3, 5, 7}\n\nfn main() {\n    view := primes[:1]\n    head := append(view, 99)\n    _ = head\n}\n",
        "local alias of a const reslice",
    );
}

#[test]
fn append_into_const_field_and_element_reslices_is_e3006() {
    assert_e3006(
        "struct cfg { ports []int }\n\nconst conf = cfg{ports: []int{80, 443, 8080}}\n\nfn main() {\n    extra := append(conf.ports[:1], 22)\n    _ = extra\n}\n",
        "const struct field reslice",
    );
    assert_e3006(
        "const grid = [][]int{[]int{1, 2}, []int{3, 4}}\n\nfn main() {\n    for _, row in grid {\n        _ = append(row[:0], 7)\n    }\n}\n",
        "range copy of a const element, resliced",
    );
}

#[test]
fn append_into_const_reslice_inside_closure_is_e3006() {
    assert_e3006(
        "const primes = []int{2, 3, 5, 7}\n\nfn main() {\n    f := fn() []int {\n        return append(primes[:1], 99)\n    }\n    _ = f()\n}\n",
        "closure body",
    );
}

#[test]
fn append_that_always_reallocates_stays_legal() {
    assert_accept(
        "const primes = []int{2, 3, 5, 7}\n\nstruct cfg { ports []int }\n\nconst conf = cfg{ports: []int{80, 443}}\n\nfn main() {\n    more := append(primes, 11)\n    tail := append(primes[1:], 13)\n    p2 := append(conf.ports, 22)\n    var local []int\n    local = append(local, primes[0])\n    _ = more\n    _ = tail\n    _ = p2\n    _ = local\n}\n",
        "full-capacity destinations reallocate",
    );
}
