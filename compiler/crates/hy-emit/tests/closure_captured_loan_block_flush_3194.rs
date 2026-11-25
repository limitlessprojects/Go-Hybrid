//! #3194: a slot loan captured by a closure inside a block / `for` / `if`
//! body must be stored back after the closure's last call, not right after
//! the closure literal (the call would otherwise write a dead copy).
use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

fn run(name: &str, src: &str) -> String {
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join(format!("hy_3194_{name}.go"));
    std::fs::write(&go_file, &go_src).expect("write go");
    let out = Command::new("go").args(["run", go_file.to_str().unwrap()]).output().expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stderr), go_src);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const HEAD: &str = "package main\nimport \"fmt\"\nstruct cell { n int }\n";

#[test]
fn for_body_closure_write_lands() {
    let src = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}, cell{{n: 1}}}}\n    for i in 0..2 {{\n        p := &xs[i]\n        bump := fn() {{ p.n += 10 }}\n        bump()\n    }}\n    fmt.println(xs[0].n, xs[1].n)\n}}\n");
    assert_eq!(run("for", &src), "11 11");
}

#[test]
fn if_body_closure_write_lands() {
    let src = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    if len(xs) > 0 {{\n        p := &xs[0]\n        bump := fn() {{ p.n = 7 }}\n        bump()\n    }}\n    fmt.println(xs[0].n)\n}}\n");
    assert_eq!(run("if", &src), "7");
}

#[test]
fn bare_block_slice_array_and_map_slot() {
    let slice = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    {{\n        p := &xs[0]\n        f := fn() {{ p.n = 2 }}\n        f()\n    }}\n    fmt.println(xs[0].n)\n}}\n");
    assert_eq!(run("slice", &slice), "2");
    let arr = format!("{HEAD}fn main() {{\n    var arr [2]cell\n    {{\n        p := &arr[0]\n        f := fn() {{ p.n = 2 }}\n        f()\n    }}\n    fmt.println(arr[0].n)\n}}\n");
    assert_eq!(run("array", &arr), "2");
    let map = format!("{HEAD}fn main() {{
    m := map[string]cell{{\"a\": cell{{n: 1}}}}
    {{
        p := &m[\"a\"]
        f := fn() {{ p.n = 2 }}
        f()
    }}
    fmt.println(m[\"a\"].n)
}}
");
    assert_eq!(run("map", &map), "2");
}

#[test]
fn closure_called_twice_and_direct_write_control() {
    let twice = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    {{\n        p := &xs[0]\n        f := fn() {{ p.n += 1 }}\n        f()\n        f()\n    }}\n    fmt.println(xs[0].n)\n}}\n");
    assert_eq!(run("twice", &twice), "3");
    let direct = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    {{\n        p := &xs[0]\n        p.n = 2\n    }}\n    fmt.println(xs[0].n)\n}}\n");
    assert_eq!(run("direct", &direct), "2");
}
