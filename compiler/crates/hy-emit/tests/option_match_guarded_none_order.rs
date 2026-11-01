use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn guarded_none_and_wildcard_arms_evaluate_guards_and_preserve_order() {
    let src = "package main\nimport \"fmt\"\nfn f(x option[int], q bool) string {\n match x {\n some(v) if v > 10 => { return \"big\" }\n _ if q => { return \"flag\" }\n some(_) => { return \"sm\" }\n none if false => { return \"bad\" }\n _ => { return \"fb\" }\n }\n}\nfn main() {\n var e option[int]\n fmt.println(f(e, false), f(some(3), true), f(some(3), false))\n}\n";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join("hy_option_match_guarded_prog.go");
    std::fs::write(&go_file, go_src).expect("write");
    let out = Command::new("go").args(["run", go_file.to_str().unwrap()]).output().expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "fb flag sm");
}
