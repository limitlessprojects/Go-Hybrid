use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

#[test]
fn indirect_callee_const_mutation_rejected_e3006() {
    for src in [
        "const c = []int{1, 2}\nfn touch(xs []int) { xs[0] = 9 }\nfn main() {\n    fs := []fn(xs []int){touch}\n    fs[0](c)\n}\n",
        "const c = map[string]int{\"a\": 1}\nfn touch(m map[string]int) { m[\"a\"] = 9 }\nfn main() {\n    fm := map[string]fn(m map[string]int){\"k\": touch}\n    fm[\"k\"](c)\n}\n",
        "const c = []int{1, 2}\nfn touch(xs []int) { xs[0] = 9 }\nfn pick() fn(xs []int) { return touch }\nfn main() { pick()(c) }\n",
        "const c = []int{1, 2}\nfn run(ch chan fn(xs []int)) { (<-ch)(c) }\n",
        "struct item { v int }\nstruct runner { f fn(p *item) }\nconst c = item{ v: 1 }\nfn touch(p *item) { p.v = 2 }\nfn main() {\n    r := runner{ f: touch }\n    r.f(c)\n}\n",
        "struct item { v int }\ntype mut_fn = fn(p *item)\nconst c = item{ v: 1 }\nfn touch(p *item) { p.v = 2 }\nfn main() {\n    var f mut_fn = touch\n    f(c)\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == "HYB-E3006"),
            "expected HYB-E3006 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }

    let ok = check(
        "struct item { v int }\nstruct runner { f fn(p *item) }\nfn touch(p *item) { p.v = 2 }\nfn main() {\n    xs := []int{1, 2}\n    fs := []fn(a *item){touch}\n    it := item{ v: 1 }\n    r := runner{ f: touch }\n    fs[0](it)\n    r.f(it)\n    _ = xs\n}\n",
    );
    assert_eq!(ok.verdict, Verdict::Accept, "unexpected diagnostics: {:?}", ok.diagnostics);
}
