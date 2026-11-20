use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn user_struct_and_foreign_shapes_across_fn_assoc_and_method_calls() {
    let src = r#"struct pool_stats { mu sync.mutex }
type stats_alias = pool_stats
struct box {
    inner pool_stats
    hits atomic.int64
    tags set[string]
}
fn (s *pool_stats) pop() int { return 0 }
fn make_stats() *pool_stats { return new(pool_stats) }
fn make_alias() *stats_alias { return new(pool_stats) }
fn make_stats_g[t any]() *pool_stats { return new(pool_stats) }
fn box.make_stats() *pool_stats { return new(pool_stats) }
fn box.make_stats_g[t any]() *pool_stats { return new(pool_stats) }
fn (b *box) get_stats() *pool_stats { return &b.inner }
fn (b *box) get_stats_g[t any]() *pool_stats { return &b.inner }
fn (b *box) get_counter() *atomic.int64 { return &b.hits }
fn (b *box) get_tags() set[string] { return b.tags }

fn main() {
    make_stats().mu.lock()
    make_alias().mu.lock()
    make_stats_g[int]().mu.lock()
    box.make_stats().mu.lock()
    box.make_stats_g[int]().mu.lock()

    b := box{}
    b.get_stats().mu.lock()
    b.get_stats_g[int]().mu.lock()
    b.get_counter().add(1)

    _ = new(pool_stats).pop()
    _ = make_stats().pop()
    _ = make_stats_g[int]().pop()
    _ = box.make_stats().pop()
    _ = b.get_stats().pop()

    s1 := make_stats()
    s1.mu.lock()
    _ = s1.pop()

    s2 := box.make_stats_g[int]()
    s2.mu.lock()

    s3 := b.get_stats()
    s3.mu.lock()
    _ = s3.pop()

    c := b.get_counter()
    c.add(1)

    tags := b.get_tags()
    _ = len(tags)
}
"#;
    let f = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&f).expect("emit");
    assert!(go.contains("make_stats().mu.Lock()"), "{go}");
    assert!(go.contains("make_alias().mu.Lock()"), "{go}");
    assert!(go.contains("make_stats_g[int]().mu.Lock()"), "{go}");
    assert!(go.contains("box_make_stats().mu.Lock()"), "{go}");
    assert!(go.contains("box_make_stats_g[int]().mu.Lock()"), "{go}");
    assert!(go.contains("b.get_stats().mu.Lock()"), "{go}");
    assert!(go.contains("b.get_stats_g[int]().mu.Lock()"), "{go}");
    assert!(go.contains("b.get_counter().Add(1)"), "{go}");
    assert!(go.contains("_ = new(pool_stats).pop()"), "{go}");
    assert!(go.contains("_ = make_stats().pop()"), "{go}");
    assert!(go.contains("_ = make_stats_g[int]().pop()"), "{go}");
    assert!(go.contains("_ = box_make_stats().pop()"), "{go}");
    assert!(go.contains("_ = b.get_stats().pop()"), "{go}");
    assert!(go.contains("s1.mu.Lock()"), "{go}");
    assert!(go.contains("_ = s1.pop()"), "{go}");
    assert!(go.contains("s2.mu.Lock()"), "{go}");
    assert!(go.contains("s3.mu.Lock()"), "{go}");
    assert!(go.contains("_ = s3.pop()"), "{go}");
    assert!(go.contains("c.Add(1)"), "{go}");
    assert!(go.contains("_ = tags.len()"), "{go}");
}
