//! SPEC-LANGUAGE.md T3 (§2.2) carries a runnable `// expect:` test-claim
//! (§13.1: the expected output is part of the rule). Until the Phase-1
//! extractor exists, this test runs THAT block straight out of the spec
//! through `hyb build` and diffs stdout against its `expect` lines, so the
//! claim and the compiler cannot drift apart again (issue #277: the claim
//! said `for c in s { fmt.println(c) }` prints `h é l l o`, while a `rune`
//! is an `i32` and prints as its code point under Go's `%v`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const SPEC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../SPEC-LANGUAGE.md");

/// The first ```hy block after the T3 rule heading.
fn t3_block(spec: &str) -> &str {
    let rule = spec.find("**T3 —").expect("SPEC-LANGUAGE.md has a T3 rule");
    let rest = &spec[rule..];
    let open = rest.find("```hy\n").expect("T3 has a hy code block") + "```hy\n".len();
    let body = &rest[open..];
    let close = body.find("```").expect("T3 code block is closed");
    &body[..close]
}

/// `// expect: 104 233   (note)` -> ["104", "233"]: the text before the
/// parenthetical note, one stdout line per whitespace-separated token.
fn expected_lines(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in block.lines() {
        if let Some((_, claim)) = line.split_once("// expect:") {
            let claim = claim.split('(').next().unwrap_or("");
            out.extend(claim.split_whitespace().map(str::to_string));
        }
    }
    out
}

#[test]
fn t3_expect_claims_match_hyb_build_output() {
    let spec = fs::read_to_string(SPEC).expect("read SPEC-LANGUAGE.md");
    let block = t3_block(&spec);
    let want = expected_lines(block);
    assert!(
        want.len() >= 3,
        "T3 block must keep its expect claims: {block}"
    );

    let dir: PathBuf = std::env::temp_dir().join(format!("hyb-spec-t3-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("t3.hy");
    fs::write(&src, format!("fn main() {{\n{block}}}\n")).unwrap();
    let bin = dir.join("t3");

    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .current_dir(&dir)
        .output()
        .expect("run hyb build");
    assert!(
        build.status.success(),
        "hyb build of the T3 spec block failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new(&bin).output().expect("run the T3 binary");
    let _ = fs::remove_dir_all(&dir);
    assert!(run.status.success(), "T3 binary exited {:?}", run.status);
    let stdout = String::from_utf8(run.stdout).expect("UTF-8 stdout");
    let got: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        got, want,
        "SPEC-LANGUAGE.md T3 expect claims disagree with the compiled program"
    );
}

#[test]
fn expect_parser_drops_the_parenthetical_note() {
    let block = "fmt.println(s[0])   // expect: 104 (byte 'h')\n\
                 for c in s { fmt.println(c) }   // expect: 104 233   (é is one rune)\n";
    assert_eq!(expected_lines(block), ["104", "104", "233"]);
}
