//! Format-consuming calls must encode interpolated literals as Go strings.

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
            "hyb-printf-escapes-{tag}-{}-{nonce}",
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

fn check_build_run(tag: &str, body: &str, expected: &[u8]) {
    let dir = Scratch::new(tag);
    let source = dir.0.join("format.hy");
    fs::write(
        &source,
        format!("fn main() {{\n    value := 7\n{body}\n}}\n"),
    )
    .unwrap();

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

    let binary = dir
        .0
        .join(format!("format{}", std::env::consts::EXE_SUFFIX));
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
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new(&binary).output().unwrap();
    assert!(
        run.status.success(),
        "program failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, expected);
}

#[test]
fn printf_interpolated_controls_preserve_exact_bytes() {
    check_build_run(
        "printf-controls",
        r#"    fmt.printf("\x00{value}\x07\x1b\v\f\u007f\n")"#,
        b"\x007\x07\x1b\x0b\x0c\x7f\n",
    );
}

#[test]
fn fprintf_interpolated_controls_preserve_exact_bytes() {
    check_build_run(
        "fprintf-controls",
        r#"    fmt.fprintf(os.stdout, "\x00{value}\x07\x1b\v\f\u007f\n")"#,
        b"\x007\x07\x1b\x0b\x0c\x7f\n",
    );
}

#[test]
fn ordinary_interpolation_and_format_calls_keep_their_semantics() {
    check_build_run(
        "ordinary",
        r#"    text := "\x00{value}\x07\x1b\v\f\u007f"
    fmt.print(text)
    fmt.printf("100% {value} \"quoted\" \\ {{braces}} hé🙂\n")
    fmt.fprintf(os.stdout, "{value}\tend\n")
    fmt.printf("%d %s\n", value, "ok")"#,
        "\x007\u{7}\u{1b}\u{b}\u{c}\u{7f}100% 7 \"quoted\" \\ {braces} hé🙂\n7\tend\n7 ok\n"
            .as_bytes(),
    );
}

#[test]
fn utf8_byte_escapes_preserve_length_indexing_and_comparison() {
    check_build_run(
        "utf8-byte-values",
        r#"    s := "\xc3\xa9"
    fmt.println(len(s))
    fmt.println(s[0])
    fmt.println(s[1])
    fmt.println(s == "é")
    fmt.println("\303\251" == "é")
    fmt.println("\xe4\xb8\xad" == "中")
    fmt.println("\xf0\x9f\x99\x82" == "🙂")
    fmt.println("prefix:\xc3\xa9:{value}" == "prefix:é:7")
    fmt.println("\x41" == "A")
    fmt.println("\u00e9" == "é")
    fmt.println("\U0001f642" == "🙂")
    fmt.println('\xe9' == '\u{e9}')"#,
        b"2\n195\n169\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
    );
}

#[test]
fn format_calls_preserve_utf8_escaped_chunks() {
    check_build_run(
        "utf8-format-chunks",
        r#"    fmt.printf("\xc3\xa9{value}\344\270\255\n")
    fmt.fprintf(os.stdout, "\xf0\x9f\x99\x82{value}\303\251\n")"#,
        "é7中\n🙂7é\n".as_bytes(),
    );
}
