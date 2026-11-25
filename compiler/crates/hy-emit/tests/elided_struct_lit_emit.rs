use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn elided_pointer_struct_elements_and_map_struct_keys_build_and_run() {
    let src = "package main\nimport \"fmt\"\nstruct point { x int\n y int }\nstruct cfg { tags map[string]int }\nfn main() {\n ps := []*point{{x: 4, y: 5}}\n cs := map[string]*cfg{\"a\": {}}\n m := map[point]int{{x: 1, y: 2}: 9}\n fmt.println(ps[0].x, cs[\"a\"].tags != nil, m[point{x: 1, y: 2}])\n}\n";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join("hy_elided_struct_lit_prog.go");
    std::fs::write(&go_file, go_src).expect("write");
    let out = Command::new("go").args(["run", go_file.to_str().unwrap()]).output().expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4 true 9");
}
