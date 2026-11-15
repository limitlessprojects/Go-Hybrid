use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn elided_literals_through_pointer_slice_and_array_type_aliases_build_and_run() {
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
type pair = [2]point
type cfg_pair = [2]cfg
fn main() {
    ps := []point_ptr{{x: 4, y: 5}}
    cs := map[string]cfg_ptr{\"a\": {}}
    cs[\"a\"].tags[\"k\"] = 7
    grid := []row{[{x: 1, y: 2}], {}}
    pairs := []pair{[{x: 3, y: 6}, {x: 8, y: 9}]}
    cp := cfg_pair{}
    cp[0].tags[\"m\"] = 11
    nested := []cfg_pair{[{}], {}}
    nested[0][1].tags[\"n\"] = 13
    nested[1][0].tags[\"p\"] = 17
    fmt.println(ps[0].x, cs[\"a\"].tags[\"k\"], grid[0][0].y, len(grid[1]), pairs[0][1].x, cp[0].tags[\"m\"], nested[0][1].tags[\"n\"], nested[1][0].tags[\"p\"])
}
";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join("hy_elided_literal_type_alias_prog.go");
    std::fs::write(&go_file, &go_src).expect("write");
    let out = Command::new("go")
        .args(["run", go_file.to_str().unwrap()])
        .output()
        .expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(
        out.status.success(),
        "{}\n---\n{}",
        String::from_utf8_lossy(&out.stderr),
        go_src
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "4 7 2 0 8 11 13 17"
    );
}
