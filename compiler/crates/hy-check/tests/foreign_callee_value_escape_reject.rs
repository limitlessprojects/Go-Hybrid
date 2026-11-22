//! A5 Go-escape check-site (`HYB-E3013`) — first-class foreign Go function and method values reject escaping mutating closures.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

const PRE: &str = "import \"net/http\"\nimport \"time\"\ntype reg_fn = fn(p string, cb fn())\nstruct runner_box { local reg_fn\n foreign reg_fn }\nfn _use_pkgs() { _ = time.second\n http.handle_func(\"/k\", fn() {}) }\nfn local_reg(p string, cb fn()) { _ = p\n cb() }\nfn pick_reg() reg_fn { return http.handle_func }\nfn named_reg() (s reg_fn) { s = http.handle_func\n return }\nfn pair_regs() (int, reg_fn) { return 1, http.handle_func }\nfn passthru(f reg_fn) reg_fn { return f }\nfn run_with(sp reg_fn, m map[string]int) { sp(\"/x\", fn() { m[\"bg\"] = 1 }) }\nfn run_both(sp reg_fn, cb fn()) { sp(\"/x\", cb) }";

#[test]
fn first_class_foreign_fn_and_method_callees_reject_e3013() {
    for body in [
        "mux := http.new_serve_mux()\nh := mux.handle_func\ncount := 0\nh(\"/a\", fn() { count += 1 })\n_ = count",
        "m := map[string]int{\"init\": 0}\nspawn_after := time.after_func\n_ = spawn_after(0, fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nf := local_reg\nif true { f = http.handle_func }\nf(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nbx := runner_box{local: local_reg, foreign: http.handle_func}\nbx.foreign(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nxs := []reg_fn{local_reg, http.handle_func}\nxs[1](\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nmp := map[string]reg_fn{\"bg\": http.handle_func}\nmp[\"bg\"](\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\npick_reg()(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nnamed_reg()(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\n_, sp := pair_regs()\nsp(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\npassthru(http.handle_func)(\"/x\", fn() { m[\"bg\"] = 1 })",
        "m := map[string]int{\"init\": 0}\nrun_with(http.handle_func, m)",
        "m := map[string]int{\"init\": 0}\nrun_both(http.handle_func, fn() { m[\"bg\"] = 1 })",
    ] {
        let r = check_file(&parse_file("t.hy", &format!("{PRE}\nfn main() {{\n{body}\n}}\n")).expect("snippet parses"));
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E3013"), "expected HYB-E3013, got {:?} for:\n{body}", r.diagnostics);
    }
}
