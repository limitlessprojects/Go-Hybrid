use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn addr_of_foreign_and_nested_struct_receivers_build_and_run() {
    let src = "\
package main
import \"fmt\"
import \"net/url\"
import \"strings\"
import \"sync\"
import \"sync/atomic\"

struct inner_box {
    mu sync.mutex
    hits atomic.int64
}

struct outer_box {
    sub inner_box
}

fn main() {
    var mu sync.mutex
    (&mu).lock()
    (&mu).unlock()

    var rw sync.rw_mutex
    (&rw).rlock()
    (&rw).runlock()

    var wg sync.wait_group
    (&wg).add(1)
    (&wg).done()
    (&wg).wait()

    var once sync.once
    (&once).do(fn() {})

    var hits atomic.int64
    (&hits).add(4)

    var o outer_box
    (&o.sub.mu).lock()
    (&o.sub.hits).add(6)
    (&o.sub.mu).unlock()

    var u url.url
    u.raw_query = \"k=7\"
    q := (&u).query.get(\"k\")

    var sb strings.builder
    (&sb).grow(8)
    grown := (&sb).cap() >= 8
    (&sb).reset()

    fmt.println((&hits).load(), (&o.sub.hits).load(), q, grown, (&sb).len())
}
";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    for expected in [
        "(&mu).Lock()",
        "(&rw).RLock()",
        "(&wg).Add(1)",
        "(&once).Do(",
        "(&hits).Add(4)",
        "(&o.sub.mu).Lock()",
        "(&o.sub.hits).Add(6)",
        "(&u).Query().Get(\"k\")",
        "(&sb).Grow(8)",
    ] {
        assert!(go_src.contains(expected), "missing `{expected}` in:\n{go_src}");
    }
    let go_file = std::env::temp_dir().join("hy_addr_of_foreign_receiver_shapes.go");
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
        "4 6 7 true 0"
    );
}
