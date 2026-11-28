//! Regression tests for `walk.rs`:
//! 1. Folded bracket index/type-argument suffixes (`fns[k](41)`, `fns[p](41)`) record both
//!    the base container (`fns`) and the bracketed key/argument (`k`, `p`) in `own_uses`
//!    and `FlatInfo::last_use` (`HYB-E3003` and `HYB-E3102`).
//! 2. `Expr::Move` on projected places (`move m[kb.k]`, `move arr[p.n]`) records uses of
//!    index and slice-bound subexpressions (`kb`, `p`) in addition to the root binding.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

#[test]
fn rejects_use_after_move_on_folded_index_callee_base_and_key() {
    let cases = [
        "import \"fmt\"\nstruct key_id {\n    id int\n}\nfn sink_key(k key_id) {\n    fmt.println(k.id)\n}\nfn bad_uam_key(fns map[key_id]fn(int) int) {\n    k := key_id{id: 1}\n    task sink_key(move k)\n    fmt.println(fns[k](41))\n}\n",
        "import \"fmt\"\nfn inc(x int) int {\n    return x + 1\n}\nfn sink_fns(fs []fn(int) int) {\n    fmt.println(len(fs))\n}\nfn bad_uam_base(k int) {\n    fns := []fn(int) int{inc}\n    managed(\"test\", \"concurrency\") {\n        task sink_fns(move fns)\n    }\n    fmt.println(fns[k](41))\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "got: {:?}", r.diagnostics);
    }
}

#[test]
fn rejects_a3_map_slot_loan_invalidation_when_loan_used_in_folded_index_callee() {
    let src = "import \"fmt\"\nfn bad_a3_loan(fns map[*int]fn(int) int) {\n    m := map[string]int{\"a\": 0}\n    p := &m[\"a\"]\n    _ = p\n    m[\"b\"] = 1\n    fmt.println(fns[p](41), len(m))\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3102"), "got: {:?}", r.diagnostics);
}

#[test]
fn rejects_uam_and_a3_loan_in_move_projection_subexpressions() {
    let src1 = "import \"fmt\"\nstruct key_box {\n    k string\n    buf []int\n}\nfn sink(b key_box) {\n    fmt.println(b.k, len(b.buf))\n}\nfn bad_uam_in_move_index() {\n    m := map[string]int{\"a\": 1}\n    kb := key_box{k: \"a\", buf: []int{1}}\n    task sink(move kb)\n    v := move m[kb.k]\n    fmt.println(v, len(m))\n}\n";
    assert!(check(src1).diagnostics.iter().any(|d| d.code == "HYB-E3003"));

    let src2 = "import \"fmt\"\nstruct item {\n    n int\n}\nfn bad_a3_in_move_index() {\n    m := map[string]item{\"a\": item{n: 0}}\n    arr := [2]int{10, 20}\n    p := &m[\"a\"]\n    m[\"b\"] = item{n: 1}\n    v := move arr[p.n]\n    fmt.println(v, len(m), len(arr))\n}\n";
    assert!(check(src2).diagnostics.iter().any(|d| d.code == "HYB-E3102"));
}

#[test]
fn accepts_valid_folded_index_callee_and_generic_call_controls() {
    let src = "import \"fmt\"\nstruct item {\n    n int\n}\nfn id[t](x t) t {\n    return x\n}\nfn ok_controls(fns []fn(int) int, k int) {\n    m := map[string]item{\"a\": item{n: 1}}\n    p := &m[\"a\"]\n    fmt.println(fns[k](41), id[int](41), p.n)\n    m[\"b\"] = item{n: 2}\n    fmt.println(len(m))\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "got: {:?}", r.diagnostics);
}
