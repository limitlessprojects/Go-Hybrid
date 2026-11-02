//! Regression tests for SPEC-LANGUAGE.md LX13 single-var map range lowering in `hy-emit`:
//! `for k in <map_expr>` must emit `for k := range ...` across user type aliases, fields,
//! indexed containers, channel receives, derefs, method calls, and nested loop bindings.

use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

fn emit_and_run(tag: &str, src: &str) -> (String, String) {
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join(format!("hy_map_key_range_{}_{tag}.go", std::process::id()));
    std::fs::write(&go_file, &go_src).expect("write go");
    let out = Command::new("go").args(["run", go_file.to_str().unwrap()]).output().expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stderr), go_src);
    (go_src, String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn aliased_field_indexed_chan_ptr_and_nested_map_ranges_bind_keys() {
    let src = "import \"fmt\"\nstruct box { val int }\ntype int_map = map[int]*box\ntype int_map2 = int_map\nconst cm = map[int]*box{1: nil}\nstruct holder { m int_map2 }\nfn (h holder) get() int_map { return h.m }\nfn main() {\n  var m int_map = (make)(int_map2)\n  m[2] = &box{val: 9}\n  m2 := m\n  h := holder{m: m}\n  ph := &h\n  ms := []int_map{m}\n  by_g := map[string]int_map{\"a\": m}\n  ch := make(chan int_map, 2)\n  ch <- m\n  ch <- m\n  f := fn() int_map { return m }\n  opt := some(m)\n  sum := 0\n  for k in cm { sum += k }\n  for k in m2 { sum += k }\n  for k in ph.m { sum += k }\n  for k in h.m { sum += k }\n  for k in ph.get() { sum += k }\n  for k in ms[0] { sum += k }\n  for k in ms[0:1][0] { sum += k }\n  for k in by_g[\"a\"] { sum += k }\n  for k in <-ch { sum += k }\n  for k in f() { sum += k }\n  for k in opt.unwrap() { sum += k }\n  select { mp := <-ch => { for k in mp { sum += k } } }\n  match opt {\n    some(mp) => { for k in mp { sum += k } }\n    none => {}\n  }\n  for sub in ms[0:1] { for k in sub { sum += k } }\n  fmt.println(sum)\n}\n";
    let (go_src, stdout) = emit_and_run("keys", src);
    assert!(!go_src.contains("_, k := range"), "unexpected value range:\n{go_src}");
    assert_eq!(stdout, "27");
}

#[test]
fn two_var_map_slice_and_cyclic_ptr_alias_negative_controls() {
    let cyc = "type ptr = *ptr\ntype a = *b\ntype b = *a\nfn g(p ptr, q a) { _ = p\n _ = q }\n";
    assert!(emit_go(&parse_file("cyc.hy", cyc).expect("parse")).is_ok());
    let src = "import \"fmt\"\nstruct box { val int }\ntype int_map = map[int]*box\nstruct holder { m int_map }\nfn main() {\n  h := holder{m: map[int]*box{3: &box{val: 4}}}\n  xs := []int{5, 6}\n  sum := 0\n  for k, v in h.m { sum += k + v.val }\n  for x in xs { sum += x }\n  fmt.println(sum)\n}\n";
    let (go_src, stdout) = emit_and_run("controls", src);
    assert!(go_src.contains("for k, v := range h.m"), "{go_src}");
    assert!(go_src.contains("for _, x := range xs"), "{go_src}");
    assert_eq!(stdout, "18");
}
