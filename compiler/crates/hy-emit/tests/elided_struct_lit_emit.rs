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

#[test]
fn aliased_element_types_and_n5_arrays_build_and_run() {
    let src = "\
package main
import \"fmt\"

struct point {
    x int
    y int
}

struct cfg {
    tags map[string]int
}

type point_ptr = *point
type cfg_ptr = *cfg
type row = []point
type pair = [2]int
type cfg_pair = [2]cfg

fn main() {
    ps := []point_ptr{{x: 4, y: 5}}
    cs := map[string]cfg_ptr{\"a\": {}}
    grid := []row{[{x: 1, y: 2}], []point{}}
    pairs := []pair{[1, 2]}
    nested := []cfg_pair{{}, [{}]}
    fmt.println(
        ps[0].x,
        cs[\"a\"].tags != nil,
        grid[0][0].x,
        len(grid[1]),
        pairs[0][1],
        nested[0][0].tags != nil,
        nested[0][1].tags != nil,
        nested[1][0].tags != nil,
        nested[1][1].tags != nil,
    )
}
";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join(format!(
        "hy_elided_alias_literals_{}.go",
        std::process::id()
    ));
    std::fs::write(&go_file, go_src).expect("write");
    let out = Command::new("go")
        .args(["run", go_file.to_str().unwrap()])
        .output()
        .expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "4 true 1 0 2 true true true true"
    );
}
