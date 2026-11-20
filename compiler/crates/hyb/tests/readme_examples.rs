//! The README is the front door: its Hybrid example must pass `hyb check`,
//! and every `hyb build` line it shows must name an input file (the CLI
//! usage requires `<file.hy>` in both binary and `--package` mode).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn readme() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../README.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
}

/// Bodies of the fenced blocks tagged `lang`, in document order.
fn fenced(doc: &str, lang: &str) -> Vec<String> {
    let open = format!("```{lang}");
    let mut blocks = Vec::new();
    let mut cur: Option<String> = None;
    for line in doc.lines() {
        match cur.as_mut() {
            None if line.trim_end() == open => cur = Some(String::new()),
            Some(_) if line.starts_with("```") => blocks.push(cur.take().unwrap()),
            Some(body) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    blocks
}

#[test]
fn readme_hy_example_passes_check() {
    let blocks = fenced(&readme(), "hy");
    assert!(!blocks.is_empty(), "README lost its hy example");
    let dir = std::env::temp_dir().join(format!("hyb-readme-example-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (i, src) in blocks.iter().enumerate() {
        let file = dir.join(format!("example{i}.hy"));
        fs::write(&file, src).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_hyb"))
            .arg("check")
            .arg(&file)
            .output()
            .expect("run actual hyb CLI");
        assert!(
            out.status.success(),
            "README hy block {i} fails `hyb check`:\n{src}\n{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn readme_build_commands_name_an_input_file() {
    let doc = readme();
    let mut seen = 0;
    for block in fenced(&doc, "sh").iter().chain(fenced(&doc, "bash").iter()) {
        for line in block.lines() {
            let cmd = line.split('#').next().unwrap_or("").trim();
            if !cmd.starts_with("hyb build ") {
                continue;
            }
            seen += 1;
            assert!(
                cmd.split_whitespace().any(|t| t.ends_with(".hy")),
                "README shows `{cmd}` without a .hy input (hyb build requires one)"
            );
        }
    }
    assert!(seen > 0, "README lost its hyb build examples");
}
