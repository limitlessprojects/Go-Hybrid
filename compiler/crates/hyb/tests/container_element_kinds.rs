//! A set / map / channel reached through an index read (`ss[0]`, `m["a"]`,
//! `grid[i][j]`), a local bound from one (`x := m["a"]`), or a range element
//! (`for s in ss`) must lower exactly like a plain local of that type:
//! `len`/`delete`/`for-in` on a set go through the prelude methods, and a
//! single-var `for k in <map>` binds the KEY (SPEC LX13).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hyb-container-elem-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn check_build_run(tag: &str, src: &str, expected: &str) {
    let dir = Scratch::new(tag);
    let source = dir.0.join("elem.hy");
    fs::write(&source, src).unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["check", "--json"])
        .arg(&source)
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "checker failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["summary"]["verdict"], "accept");

    let binary = dir.0.join(format!("elem{}", std::env::consts::EXE_SUFFIX));
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new(&binary).output().unwrap();
    assert!(
        run.status.success(),
        "program failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
}

#[test]
fn set_builtins_on_indexed_set_elements_build_and_run() {
    check_build_run(
        "sets",
        r#"struct holder { sets []set[int] }

fn total(ss []set[int]) int {
    n := 0
    for s in ss {
        n = n + len(s)
    }
    return n
}

fn main() {
    ss := []set[int]{set[int]{}, set[int]{}}
    ss[0].insert(1)
    ss[1].insert(2)
    ss[1].insert(3)
    fmt.println(len(ss[0]), len(ss[1]), total(ss))
    delete(ss[1], 2)
    for v in ss[1] {
        fmt.println("v", v)
    }

    var arr [2]set[int]
    arr[0].insert(9)
    fmt.println(len(arr[0]))

    m := map[string]set[int]{"a": ss[0]}
    x := m["a"]
    fmt.println(len(m["a"]), len(x), len(m["missing"]))

    grid := [][]set[int]{ss}
    row := grid[0]
    fmt.println(len(grid[0][1]), len(row[0]))

    h := holder{sets: ss}
    fmt.println(len(h.sets[0]))
}
"#,
        "1 2 3\nv 3\n1\n1 1 0\n1 1\n1\n",
    );
}

#[test]
fn single_var_range_over_indexed_map_binds_key() {
    check_build_run(
        "maps",
        r#"struct bag { m map[string]int }

fn main() {
    ms := []map[string]int{map[string]int{"a": 1}}
    for k in ms[0] {
        fmt.println("key", k)
    }
    mm := map[string]map[string]int{"x": map[string]int{"b": 2}}
    for k in mm["x"] {
        fmt.println("key", k)
    }
    inner := mm["x"]
    for k in inner {
        fmt.println("key", k)
    }
    for name, in2 in mm {
        for k in in2 {
            fmt.println(name, "key", k)
        }
    }
    var arr [1]map[string]int
    arr[0] = map[string]int{"c": 3}
    for k in arr[0] {
        fmt.println("key", k)
    }
    bs := []bag{bag{m: map[string]int{"z": 5}}}
    for k in bs[0].m {
        fmt.println("key", k)
    }
    for b in bs {
        for k in b.m {
            fmt.println("key", k)
        }
    }
    chs := []chan int{make(chan int, 1)}
    chs[0] <- 7
    close(chs[0])
    for v in chs[0] {
        fmt.println("recv", v)
    }
}
"#,
        "key a\nkey b\nkey b\nx key b\nkey c\nkey z\nkey z\nrecv 7\n",
    );
}
