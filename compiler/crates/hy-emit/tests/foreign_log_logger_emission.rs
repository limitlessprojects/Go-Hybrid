use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn test_log_logger_method_surface_emits_go_spellings_and_builds() {
    let src = r#"package main
import "log"
import "os"

fn configure(lg *log.logger) error {
    lg.set_flags(log.lstdflags | log.lutc | log.lmicroseconds)
    lg.set_prefix("[app] ")
    lg.set_output(os.stderr)
    _ = lg.flags()
    _ = lg.prefix()
    _ = lg.writer()
    lg.print("boot")
    lg.printf("port=%d", 8080)
    lg.println("ready")
    return lg.output(2, "traced")
}

fn uncalled_fatal_and_panic_helpers(lg *log.logger) {
    if false {
        lg.fatal("fatal")
        lg.fatalf("fatal=%d", 1)
        lg.fatalln("fatalln")
        lg.panic("panic")
        lg.panicf("panic=%d", 1)
        lg.panicln("panicln")
    }
}

fn main() {
    lg := log.new(os.stderr, "[init] ", log.lstdflags)
    def := log.default()
    lg.println("init")
    def.println("default")
    _ = configure(nil)
    uncalled_fatal_and_panic_helpers(nil)
}
"#;

    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit");

    for expected in [
        "log.New(os.Stderr, \"[init] \", log.LstdFlags)",
        "log.Default()",
        "lg.SetFlags(",
        "lg.SetPrefix(\"[app] \")",
        "lg.SetOutput(os.Stderr)",
        "lg.Flags()",
        "lg.Prefix()",
        "lg.Writer()",
        "lg.Print(\"boot\")",
        "lg.Printf(\"port=%d\", 8080)",
        "lg.Println(\"ready\")",
        "lg.Output(2, \"traced\")",
        "lg.Fatal(\"fatal\")",
        "lg.Fatalf(\"fatal=%d\", 1)",
        "lg.Fatalln(\"fatalln\")",
        "lg.Panic(\"panic\")",
        "lg.Panicf(\"panic=%d\", 1)",
        "lg.Panicln(\"panicln\")",
    ] {
        assert!(
            go.contains(expected),
            "expected emitted Go to contain `{expected}`, got:\n{go}"
        );
    }

    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join("test_foreign_log_logger_emission.go");
    std::fs::write(&file_path, go.as_bytes()).expect("write go file");

    let out = Command::new("go")
        .args(["build", "-o", "/dev/null"])
        .arg(&file_path)
        .output()
        .expect("execute go build");

    let _ = std::fs::remove_file(&file_path);

    assert!(
        out.status.success(),
        "go build failed on emitted Go:\n---\n{go}\n---\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
