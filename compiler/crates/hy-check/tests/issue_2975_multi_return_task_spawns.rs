use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn multi_return_task_spawn_sharing_mutable_map_rejects_e3001() {
    let src = "\
fn leak_map(m map[string]int) (int, map[string]int) {
    return 1, m
}
fn worker(id int, m map[string]int) {
    m[\"k\"] = id
}
fn main() {
    m := map[string]int{\"k\": 0}
    task worker(leak_map(m))
    m[\"k\"] = 9
}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for multi-return task spawn, got {found:?}"
    );
}

#[test]
fn multi_return_task_spawn_unproven_generic_return_rejects_e3005() {
    let src = "\
fn make_unknown[t](v t) (int, t) {
    return 1, v
}
fn worker[t](id int, val t) {
}
fn run[t](v t) {
    task worker[t](make_unknown[t](v))
}
fn main() {}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005 for unproven generic return crossing boundary, got {found:?}"
    );
}

#[test]
fn generic_task_spawn_pointer_auto_addressing_rejects_e3001() {
    let src = "\
struct stats { n int }
fn worker[t](s *stats, tag t) { s.n = 1 }
fn main() {
    const s = stats{n: 0}
    task worker[int](s, 1)
}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for auto-addressed pointer parameter on generic task, got {found:?}"
    );
}

#[test]
fn generic_task_spawn_instantiated_nil_rejects_e4001() {
    let src = "\
fn worker[t](tag t) {}
fn main() {
    task worker[[]int](nil)
}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for nil passed to non-nilable generic slice parameter, got {found:?}"
    );
}

#[test]
fn generic_task_spawn_inferred_nil_rejects_e4001() {
    let src = "\
fn worker[t](first t, second t) {}
fn main() {
    task worker([]int{1}, nil)
}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for nil passed to inferred non-nilable generic parameter, got {found:?}"
    );
}

#[test]
fn try_unwrapped_multi_return_preserves_recv_direction_rejects_close_e3009() {
    let src = "\
fn open_pair(rc <-chan int) (<-chan int, int, error) {
    return rc, 1, nil
}
fn run(rc <-chan int) error {
    ch, _ := open_pair(rc)?
    close(ch)
    return nil
}
fn main() {}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E3009"),
        "expected HYB-E3009 for close() on <-chan int from ?-unwrapped call, got {found:?}"
    );
}

#[test]
fn try_unwrapped_multi_return_preserves_recv_direction_rejects_send_e3008() {
    let src = "\
fn open_pair(rc <-chan int) (<-chan int, int, error) {
    return rc, 1, nil
}
fn run(rc <-chan int) error {
    ch, _ := open_pair(rc)?
    ch <- 7
    return nil
}
fn main() {}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for sending to <-chan int from ?-unwrapped call, got {found:?}"
    );
}

#[test]
fn try_unwrapped_multi_return_preserves_recv_direction_rejects_coercion_e3012() {
    let src = "\
fn open_pair(rc <-chan int) (<-chan int, int, error) {
    return rc, 1, nil
}
fn run(rc <-chan int) error {
    var sc chan<- int
    sc, _ = open_pair(rc)?
    return nil
}
fn main() {}
";
    let found = codes(src);
    assert!(
        found.iter().any(|c| c == "HYB-E3012"),
        "expected HYB-E3012 for coercing <-chan int to chan<- int, got {found:?}"
    );
}

#[test]
fn multi_return_task_spawn_sendable_accepts() {
    let src = "\
fn pair() (int, string) {
    return 1, \"ok\"
}
fn worker(id int, msg string) {
}
fn main() {
    task worker(pair())
}
";
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected 0 errors for sendable multi-return task spawn, got {found:?}"
    );
}
