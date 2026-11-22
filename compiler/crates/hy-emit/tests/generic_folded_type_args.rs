use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn generic_folded_array_fn_type_args_and_composite_comparable_keys_build_and_run() {
    let src = "\
package main
import \"fmt\"

struct box[t] {
    v t
}

struct pair[a, b] {
    first a
    second b
}

struct composite_keys[k, v] {
    by_arr map[[2]k]v
    by_pair set[pair[k, int]]
    nested box[set[k]]
}

fn id[t](x t) t {
    return x
}

fn apply_multi(cb fn(x u8) u16 bool, v u8) u16 {
    res, ok := cb(v)
    if ok {
        return res
    }
    return 0
}

fn main() {
    arr := [2]u8{4, 9}
    b := box[[2]u8]{v: arr}
    got := id[[2]u8](b.v)

    var fn_box box[fn(x u8) u16]
    fn_box.v = fn(x u8) u16 { return u16(x) + 10 }
    multi := apply_multi(fn(x u8) (u16, bool) { return u16(x) * 2, true }, got[0])

    var ck composite_keys[u8, int]
    ck.by_arr = map[[2]u8]int{got: int(fn_box.v(got[1]))}
    ck.by_pair.insert(pair[u8, int]{first: got[0], second: int(multi)})
    ck.nested.v.insert(got[1])

    fmt.println(ck.by_arr[got], ck.by_pair.contains(pair[u8, int]{first: 4, second: 8}), ck.nested.v.contains(9))
}
";
    let file = parse_file("test.hy", src).expect("parse");
    let go_src = emit_go(&file).expect("emit");
    let go_file = std::env::temp_dir().join("hy_generic_folded_types_prog.go");
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
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "19 true true");
}
