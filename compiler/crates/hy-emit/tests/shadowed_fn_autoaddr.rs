use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

fn emit_and_run_go(name: &str, src: &str) -> (String, String) {
    let ast = parse_file("test.hy", src).expect("source must parse");
    let go_src = emit_go(&ast).expect("source must emit");
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "hy_shadowed_fn_autoaddr_{}_{}_{}",
        name,
        std::process::id(),
        id
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let go_file = dir.join("main.go");
    std::fs::write(&go_file, &go_src).expect("write main.go");
    let out = Command::new("go")
        .args(["run", go_file.to_str().unwrap()])
        .output()
        .expect("invoke go run");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "go run failed:\n{}\n--- emitted Go ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        go_src
    );
    (
        go_src,
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
    )
}

#[test]
fn shadowed_ptr_param_fn_does_not_rewind_single_iter_loop_or_mutate_let() {
    let src = r#"import "fmt"

fn step(p *int) {
    _, _ = fmt.sscan("0", p)
}

fn worker(m map[int]int, done chan int) {
    for j in 0..2000 {
        m[j % 8] = j
    }
    done <- m[3]
}

fn main() {
    let immutable = 7
    step := fn(v any) string {
        _, _ = fmt.sscan("-1", v)
        return fmt.sprintf("%T", v)
    }
    dyn_ty := step(immutable)

    m := map[int]int{0: 1}
    done := make(chan int, 2)
    runs := 0
    for i in 0..1 {
        if runs == 0 {
            _ = step(i)
        }
        runs = runs + 1
        task worker(move m, done)
    }
    got := <-done
    fmt.println(immutable, dyn_ty, runs, got > 0)
}
"#;
    let (go_src, stdout) = emit_and_run_go("single_iter_and_let", src);
    assert!(!go_src.contains("step(&immutable)"), "{go_src}");
    assert!(!go_src.contains("step(&i)"), "{go_src}");
    assert_eq!(stdout.trim(), "7 int 1 true");
}

#[test]
fn all_local_binders_shadow_top_level_ptr_fn_and_restore_outer_scope() {
    let src = r#"import "fmt"

type step_fn = fn(x int) int

fn step(p *int) {
    _, _ = fmt.sscan("55", p)
}

fn bump_g[t any](p *t) {
    _, _ = fmt.sscan("77", p)
}

fn via_param(step step_fn, v int) int {
    return step(v)
}

fn via_named_result(v int) (step step_fn) {
    step = fn(x int) int {
        return x + 10
    }
    _ = step(v)
    return
}

fn main() {
    {
        step := fn(x int) {
            _ = x
        }
        a := 4
        step(a)
        defer step(a)
        task step(a)
        var step_var step_fn = fn(x int) int { return x }
        _ = step_var(a)
    }

    // Outer scope restored: `step` resolves back to top-level `fn step(p *int)`.
    b := 1
    step(b)

    c := 2
    bump_g[int](c)

    via_clos := fn(step step_fn, v int) int {
        return step(v)
    }
    r1 := via_param(fn(x int) int { return x * 3 }, 6)
    r2 := via_clos(fn(x int) int { return x + 4 }, 6)
    nr := via_named_result(5)
    r3 := nr(5)

    opt := some(fn(x int) int { return x + 9 })
    r4 := 0
    match opt {
        some(step): {
            q := 8
            r4 = step(q)
        }
        none: {}
    }

    if step := fn(x int) int { return x + 2 }; step(b) > 0 {
        r4 = r4 + step(b)
    }

    fmt.println(b, c, r1, r2, r3, r4)
}
"#;
    let (go_src, stdout) = emit_and_run_go("binders_and_restore", src);
    assert!(go_src.contains("step(&b)"), "{go_src}");
    assert!(go_src.contains("bump_g[int](&c)"), "{go_src}");
    assert_eq!(stdout.trim(), "55 77 18 10 15 74");
}

#[test]
fn shadowed_assoc_namespace_and_ptr_returning_fns_emit_sound_go() {
    let src = r#"import "fmt"

struct cell {
    v int
}

struct box {
    v int
}

fn box.get() int {
    return -1
}

fn box.make_ptr() *cell {
    return new(cell)
}

fn (b box) get() int {
    return b.v
}

fn (b box) ptr() *cell {
    return new(cell)
}

fn ptr() cell {
    return cell{v: 3}
}

fn get_ptr() *cell {
    return new(cell)
}

fn bump(p *cell) {
    p.v = 9
}

fn read_param(box box) int {
    return box.get()
}

fn (box box) read_self() int {
    return box.get()
}

fn main() {
    assoc_v := box.get()
    ap := box.make_ptr()
    bump(ap)

    b0 := box{v: 42}
    mp := b0.ptr()
    bump(mp)

    // Receiver method `(box) ptr() *cell` must not poison top-level `ptr() cell`.
    plain_v := ptr()
    bump(plain_v)

    // Shadowing top-level `get_ptr() *cell` with a value-returning closure must
    // not poison `local_v` into `pointer_vars`.
    {
        get_ptr := fn() cell {
            return cell{v: 4}
        }
        local_v := get_ptr()
        bump(local_v)
        fmt.print(local_v.v, " ")
    }

    // After block exit, unshadowed `get_ptr() *cell` still returns a pointer.
    real_p := get_ptr()
    bump(real_p)

    // Local `box` shadows the associated-fn namespace `box.*` and calls method `get()`.
    box := box{v: 42}
    fmt.println(assoc_v, ap.v, mp.v, plain_v.v, real_p.v, box.get(), read_param(box), box.read_self())
}
"#;
    let (go_src, stdout) = emit_and_run_go("assoc_and_ptr_ret", src);
    assert!(go_src.contains("box_get()"), "{go_src}");
    assert!(go_src.contains("box.get()"), "{go_src}");
    assert!(go_src.contains("bump(ap)"), "{go_src}");
    assert!(go_src.contains("bump(mp)"), "{go_src}");
    assert!(go_src.contains("bump(&plain_v)"), "{go_src}");
    assert!(go_src.contains("bump(&local_v)"), "{go_src}");
    assert!(go_src.contains("bump(real_p)"), "{go_src}");
    assert_eq!(stdout.trim(), "9 -1 9 9 9 9 42 42 42");
}

#[test]
fn inner_block_closure_shadow_restores_outer_try_call_shape() {
    let src = r#"import "fmt"

fn step() (int, error) {
    return 41, nil
}

fn main() {
    {
        step := fn() error {
            return nil
        }
        step()?
    }
    got := step()? + 1
    fmt.println(got)
}
"#;
    let (_, stdout) = emit_and_run_go("closure_shape_restore", src);
    assert_eq!(stdout.trim(), "42");
}

