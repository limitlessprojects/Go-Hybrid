use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

#[test]
fn composite_closure_holders_preserve_capture_freeze_across_rebinds_e3003() {
    let hdr = "struct runner { run fn() }\nfn (r *runner) set(f fn()) { r.run = f }\nfn id_runner(r runner) runner { return r }\nfn make_pair() ([]int, int) { return []int{2}, 1 }\nfn worker(x []int, done chan int) { x[0] = 1\n done <- 1 }\nfn arr_worker(x [2]int, done chan int) { done <- x[0] }\n";
    let bodies = [
        "s := []int{0}\n r := runner{run: fn() { s[0] = 99 }}\n s = []int{1}\n task worker(move s, done)\n r.run()",
        "s := []int{0}\n var r runner = runner{run: fn() {}}\n r.run = fn() { s[0] = 99 }\n s = []int{1}\n task worker(move s, done)\n r.run()",
        "s := []int{0}\n fns := []fn(){fn() { s[0] = 99 }}\n s = []int{1}\n task worker(move s, done)\n fns[0]()",
        "s := []int{0}\n var fns []fn() = []fn(){}\n fns = append(fns, fn() { s[0] = 99 })\n dst := make([]fn(), 1)\n copy(dst, fns)\n s = []int{1}\n task worker(move s, done)\n dst[0]()",
        "s := []int{0}\n arr := [1]fn(){fn() { s[0] = 99 }}\n s = []int{1}\n task worker(move s, done)\n arr[0]()",
        "s := []int{0}\n mp := map[string]fn(){\"f\": fn() { s[0] = 99 }}\n s = []int{1}\n task worker(move s, done)\n mp[\"f\"]()",
        "s := []int{0}\n f := fn() { s[0] = 99 }\n r := id_runner(runner{run: f})\n r2 := r\n r = runner{run: fn() {}}\n s = []int{1}\n task worker(move s, done)\n r2.run()",
        "s := []int{0}\n var r runner = runner{run: fn() {}}\n r.set(fn() { s[0] = 99 })\n s = []int{1}\n task worker(move s, done)\n r.run()",
        "s := []int{0}\n var tag int = 0\n r := runner{run: fn() { s[0] = 99 }}\n s, tag = make_pair()\n _ = tag\n task worker(move s, done)\n r.run()",
        "ch := make(chan []int, 1)\n ch <- move []int{2}\n s := []int{0}\n var ok bool = false\n r := runner{run: fn() { s[0] = 99 }}\n s, ok = <-ch\n _ = ok\n task worker(move s, done)\n r.run()",
        "s := []int{0}\n r := runner{run: fn() { s[0] = 99 }}\n s, tag := []int{1}, 1\n _ = tag\n task worker(move s, done)\n r.run()",
        "var arr [2]int = [2]int{1, 2}\n r := runner{run: fn() { arr[0] = 99 }}\n arr = [2]int{3, 4}\n task arr_worker(move arr, done)\n r.run()",
    ];
    for (idx, body) in bodies.iter().enumerate() {
        let src = format!("{hdr}fn main() {{\n done := make(chan int, 1)\n {body}\n _ = <-done\n}}\n");
        let codes: Vec<_> = check_file(&parse_file("test.hy", &src).expect("parse"))
            .diagnostics.into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.code).collect();
        assert!(codes.iter().any(|c| c == "HYB-E3003"), "case {idx} got {codes:?}");
    }
}
