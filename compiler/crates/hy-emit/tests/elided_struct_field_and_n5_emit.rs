use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn elided_nested_struct_fields_generic_fields_and_shadowed_make_build_and_run() {
    let src = "package main\nimport \"fmt\"\nconst k = 3\nstruct bag { counts map[string]int\n label string }\ntype bags = []bag\nstruct box[t any] { item t }\nstruct shelf { top bag\n ptr *bag\n items []bag\n arr [0x2]bag\n carr [k]bag }\nfn main() {\n s := shelf{top: {label: \"a\"}, ptr: {label: \"b\"}, items: {{label: \"c\"}}, arr: {{label: \"d\"}}, carr: {{label: \"e\"}}}\n s.top.counts[\"k\"] = 1\n s.ptr.counts[\"k\"] = 2\n s.items[0].counts[\"k\"] = 3\n s.arr[1].counts[\"k\"] = 4\n s.carr[2].counts[\"k\"] = 5\n b := box[bag]{item: {label: \"g\"}}\n b.item.counts[\"k\"] = 6\n bags := make([]int, 2)\n fmt.println(s.top.counts[\"k\"], s.ptr.counts[\"k\"], s.items[0].counts[\"k\"], s.arr[1].counts[\"k\"], s.carr[2].counts[\"k\"], b.item.counts[\"k\"], len(bags))\n}\n";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    assert!(!go_src.contains("hy_n5_make"), "shadowed slice alias must not trigger hy_n5_make:\n{go_src}");
    let go_file = std::env::temp_dir().join("hy_elided_struct_field_n5_prog.go");
    std::fs::write(&go_file, &go_src).expect("write");
    let out = Command::new("go").args(["run", go_file.to_str().unwrap()]).output().expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stderr), go_src);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1 2 3 4 5 6 2");
}
