use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn expr_struct_and_foreign_shapes_across_move_new_and_calls() {
    let src = r#"struct pool_stats { mu sync.mutex }
struct box {}
fn (s *pool_stats) pop() int { return 0 }
fn make_counter[t any]() *atomic.int64 { return new(atomic.int64) }
fn box.make_counter() *atomic.int64 { return new(atomic.int64) }
fn box.make_counter_g[t any]() *atomic.int64 { return new(atomic.int64) }
fn main() {
    s1 := pool_stats{}
    s2 := move s1
    s2.mu.lock()
    _ = s2.pop()
    new(pool_stats{}).mu.lock()
    new(atomic.int64{}).add(1)
    make_counter[int]().add(1)
    box.make_counter().add(1)
    box.make_counter_g[int]().add(1)
}
"#;
    let f = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&f).expect("emit");
    assert!(go.contains("s2.mu.Lock()"), "{go}");
    assert!(go.contains("_ = s2.pop()"), "{go}");
    assert!(go.contains("new(pool_stats).mu.Lock()"), "{go}");
    assert!(go.contains("new(atomic.Int64).Add(1)"), "{go}");
    assert!(go.contains("make_counter[int]().Add(1)"), "{go}");
    assert!(go.contains("box_make_counter().Add(1)"), "{go}");
    assert!(go.contains("box_make_counter_g[int]().Add(1)"), "{go}");
}
