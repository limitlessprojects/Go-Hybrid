//! A5 Go-escape check-site (`HYB-E3013`) — local Hybrid function values, read-only captures, and `managed` blocks stay accepted.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

const PRE: &str = "import \"net/http\"\nimport \"time\"\ntype reg_fn = fn(p string, cb fn())\nstruct runner_box { local reg_fn\n foreign reg_fn }\nfn _use_pkgs() { _ = time.second\n http.handle_func(\"/k\", fn() {}) }\nfn local_reg(p string, cb fn()) { _ = p\n cb() }\nfn passthru(f reg_fn) reg_fn { return f }\nfn run_with(sp reg_fn, m map[string]int) { sp(\"/x\", fn() { m[\"bg\"] = 1 }) }\nfn run_both(sp reg_fn, cb fn()) { sp(\"/x\", cb) }";

#[test]
fn pure_hybrid_callee_managed_interop_and_readonly_capture_accept() {
    for body in [
        "x := 0\nf := local_reg\nf(\"/x\", fn() { x = 1 })\n_ = x",
        "m := map[string]int{\"init\": 0}\nf := http.handle_func\nf = local_reg\nf(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nbx := runner_box{local: local_reg, foreign: http.handle_func}\nbx.local(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nxs := []reg_fn{local_reg, http.handle_func}\nxs[0](\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\n_ = passthru(http.handle_func)\npassthru(local_reg)(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nrun_with(local_reg, m)\nrun_both(local_reg, fn() { m[\"bg\"] = 1 })\nrun_both(http.handle_func, fn() {})",
        "m := map[string]int{\"init\": 0}\nspawn_after := time.after_func\nmanaged(\"timer\", \"interop\", \"#1\") { _ = spawn_after(0, fn() { m[\"bg\"] = 1 }) }",
        "const limit = 42\nspawn_after := time.after_func\n_ = spawn_after(0, fn() { _ = limit })",
    ] {
        let r = check_file(&parse_file("t.hy", &format!("{PRE}\nfn main() {{\n{body}\n}}\n")).expect("snippet parses"));
        assert_eq!((r.verdict, r.diagnostics.is_empty()), (Verdict::Accept, true), "unexpected {:?} for:\n{body}", r.diagnostics);
    }
}
