use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn shadowed_make_new_primitive_len_and_delete_callees_build_and_run() {
    let src = "\
package main
import \"fmt\"
struct box {
    v int
}
fn apply(new fn(b box) int, u8 fn(x int) int) int {
    return new(box{}) + (u8)(300)
}
fn main() {
    base_slice := make([]int{}, 2)
    base_slice_len := len(base_slice)
    base_ptr := new(box{})
    base_u8 := u8(300)
    var s set[int]
    s.insert(10)
    s.insert(30)
    delete(s, 30)
    base_len := len(s)

    new := fn(b box) int {
        return b.v + 5
    }
    make := fn(xs []int, n int) int {
        return len(xs) + n
    }
    u8 := fn(x int) int {
        return x + 1
    }
    picked := 0
    len := fn(x set[int]) int {
        return x.len() + 98
    }
    delete := fn(x set[int], v int) {
        picked = x.len() + v
    }

    var a int = new(box{})
    var b int = (make)([]int{}, 3)
    var c int = u8(300)
    var d int = len(s)
    delete(s, 10)
    var e int = apply(new, u8)
    fmt.println(base_slice_len, base_ptr.v, base_u8, base_len, a, b, c, d, picked, s.len(), e)
}
";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join("hy_shadowed_builtin_calls_prog.go");
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
        "2 0 44 1 5 3 301 99 11 1 306"
    );
}
