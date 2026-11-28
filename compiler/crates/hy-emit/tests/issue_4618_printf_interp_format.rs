use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn issue_4618_repro_splices_format_position_and_preserves_verbs_and_wrapping() {
    let go = emit_src(
        r#"
import "log"
import "os"

fn main() {
    name := "100%d"
    n := 5
    fmt.printf("{name} n=%d\n", n)
    p := "50%"
    fmt.println(fmt.sprintf("rate: {p}"))
    a := 3
    fmt.printf("[%s]\n", "{a}")
    base := errors.new("base")
    e := fmt.errorf("ctx {p}: %w", base)
    fmt.println(e, errors.is(e, base))
    log.set_output(os.stdout)
    log.set_flags(0)
    log.printf("pkg {p}")
}
"#,
    );

    assert!(
        go.contains(r#"fmt.Printf("%[1]v n=%[2]d\n", name, n)"#),
        "{go}"
    );
    assert!(go.contains(r#"fmt.Sprintf("rate: %v", p)"#), "{go}");
    assert!(!go.contains("fmt.Sprintf(fmt.Sprintf("), "{go}");
    assert!(
        go.contains(r#"fmt.Printf("[%s]\n", fmt.Sprintf("%v", a))"#),
        "{go}"
    );
    assert!(
        go.contains(r#"fmt.Errorf("ctx %[1]v: %[2]w", p, base)"#),
        "{go}"
    );
    assert!(go.contains(r#"log.Printf("pkg %v", p)"#), "{go}");
}

#[test]
fn printf_family_positions_cover_fprintf_appendf_eprintf_and_logger_methods() {
    let go = emit_src(
        r#"
import "log"
import "os"

struct holder {
    logger log.logger
}

fn check(buf []byte, tag string, n int, f float64, w int, x int, a int, lp *log.logger, h *holder) {
    fmt.fprintf(os.stdout, "{tag} v=%-4d r=%5.2f w=%*d pct=%%\n", n, f, w, x)
    _ = fmt.appendf(buf, "k={tag} %#04x", n)
    fmt.eprintf("err {tag}: %s\n", "{a}")
    log.fatalf("fatal {tag}: %d", n)
    log.panicf("panic {tag}")
    l := log.new(os.stdout, "", 0)
    l.printf("inst {tag}: %d", n)
    lp.printf("param {tag}: %d", n)
    log.new(os.stdout, "", 0).printf("inline {tag}")
    h.logger.printf("field {tag}: %d", n)
}
"#,
    );

    let want_fprintf = r#"fmt.Fprintf(os.Stdout, "%[1]v v=%-4[2]d r=%5.2[3]f w=%[4]*[5]d pct=%%\n", tag, n, f, w, x)"#;
    assert!(go.contains(want_fprintf), "{go}");
    assert!(
        go.contains(r#"fmt.Appendf(buf, "k=%[1]v %#04[2]x", tag, n)"#),
        "{go}"
    );
    let want_eprintf = r#"fmt.Fprintf(os.Stderr, "err %[1]v: %[2]s\n", tag, fmt.Sprintf("%v", a))"#;
    assert!(go.contains(want_eprintf), "{go}");
    assert!(
        go.contains(r#"log.Fatalf("fatal %[1]v: %[2]d", tag, n)"#),
        "{go}"
    );
    assert!(go.contains(r#"log.Panicf("panic %v", tag)"#), "{go}");
    assert!(
        go.contains(r#"l.Printf("inst %[1]v: %[2]d", tag, n)"#),
        "{go}"
    );
    assert!(
        go.contains(r#"lp.Printf("param %[1]v: %[2]d", tag, n)"#),
        "{go}"
    );
    assert!(
        go.contains(r#"log.New(os.Stdout, "", 0).Printf("inline %v", tag)"#),
        "{go}"
    );
    assert!(
        go.contains(r#"h.logger.Printf("field %[1]v: %[2]d", tag, n)"#),
        "{go}"
    );
}

#[test]
fn explicit_arg_indexes_try_hoisting_and_user_methods_stay_sound() {
    let go = emit_src(
        r#"
struct sink {}

fn (s *sink) printf(msg string) {}
fn tag() string { return "100%d" }
fn fallible() (int, error) { return 7, nil }

fn run(a string, b string, n int, prec int, val float64) error {
    _ = fmt.sprintf("{a}-{b}: %[1]d/%[1]x", n)
    _ = fmt.sprintf("{a}: %.*f", prec, val)
    _ = fmt.sprintf(("pre=%s {a} mid=%d {b} suf=%t"), "ok", n, true)
    fmt.printf("{tag()} n=%d\n", fallible()?)
    fmt.printf("{{literal}} %d\n", n)
    s := sink{}
    s.printf("{a}")
    _ = fmt.sprintf("%{a}d", n)
    _ = fmt.sprintf("{a} %d %d", n)
    return nil
}
"#,
    );

    assert!(
        go.contains(r#"fmt.Sprintf("%[1]v-%[2]v: %[3]d/%[3]x", a, b, n)"#),
        "{go}"
    );
    assert!(
        go.contains(r#"fmt.Sprintf("%[1]v: %.[2]*[3]f", a, prec, val)"#),
        "{go}"
    );
    let want_interleaved =
        r#"fmt.Sprintf("pre=%[3]s %[1]v mid=%[4]d %[2]v suf=%[5]t", a, b, "ok", n, true)"#;
    assert!(go.contains(want_interleaved), "{go}");
    assert!(go.contains("hyT0 := tag()"), "{go}");
    assert!(
        go.contains(r#"fmt.Printf("%[1]v n=%[2]d\n", hyT0, hyT1)"#),
        "{go}"
    );
    assert!(!go.contains("hyT0 := fmt.Sprintf("), "{go}");
    assert!(go.contains(r#"fmt.Printf("{literal} %d\n", n)"#), "{go}");
    assert!(go.contains(r#"s.printf(fmt.Sprintf("%v", a))"#), "{go}");
    assert!(
        go.contains(r#"fmt.Sprintf(fmt.Sprintf("%%%vd", a), n)"#),
        "{go}"
    );
    assert!(
        go.contains(r#"fmt.Sprintf(fmt.Sprintf("%v %%d %%d", a), n)"#),
        "{go}"
    );
}
