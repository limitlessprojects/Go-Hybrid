//! Tests for curated foreign method surfaces of `os.file` / `os.File`.
//! Guarantees that calling phantom/unsupported methods rejects fail-closed with HYB-E1002 (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_os_file_methods_are_rejected() {
    let bad = check(
        "import \"os\"\n\
         struct holder {\n\
             f os.file\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.f.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("os.file"),
        "message should name os.file: {}",
        diag.message
    );
    assert!(
        diag.message.contains("totally_fake_method"),
        "message should name the missing method: {}",
        diag.message
    );
}

#[test]
fn pointer_os_file_phantom_method_is_rejected() {
    let bad = check(
        "import \"os\"\n\
         fn inspect(f *os.file) {\n\
             f.phantom_call()\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("os.file"),
        "message: {}",
        diag.message
    );
    assert!(
        diag.message.contains("phantom_call"),
        "message: {}",
        diag.message
    );
}

#[test]
fn package_level_os_functions_called_as_methods_are_rejected() {
    // os.open and os.create are package-level functions, NOT methods on a File
    let bad_open = check(
        "import \"os\"\n\
         fn inspect(f *os.file) {\n\
             f.open()\n\
         }\n",
    );
    assert_eq!(bad_open.verdict, Verdict::Reject);
    assert!(codes(&bad_open).iter().any(|c| c == "HYB-E1002"));

    let bad_create = check(
        "import \"os\"\n\
         fn inspect(f *os.file) {\n\
             f.create()\n\
         }\n",
    );
    assert_eq!(bad_create.verdict, Verdict::Reject);
    assert!(codes(&bad_create).iter().any(|c| c == "HYB-E1002"));

    let bad_remove = check(
        "import \"os\"\n\
         fn inspect(f *os.file) {\n\
             f.remove()\n\
         }\n",
    );
    assert_eq!(bad_remove.verdict, Verdict::Reject);
    assert!(codes(&bad_remove).iter().any(|c| c == "HYB-E1002"));
}

#[test]
fn real_os_file_methods_accept() {
    let good = check(
        "import \"os\"\n\
         struct holder {\n\
             f os.file\n\
         }\n\
         fn run(h *holder) {\n\
             h.f.close()\n\
             h.f.sync()\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        good.diagnostics
    );
}

#[test]
fn all_real_pointer_os_file_methods_accept() {
    let good = check(
        "import \"os\"\n\
         fn inspect(f *os.file) {\n\
             f.chdir()\n\
             f.chmod()\n\
             f.chown()\n\
             f.close()\n\
             f.fd()\n\
             f.name()\n\
             f.read()\n\
             f.read_at()\n\
             f.read_dir()\n\
             f.read_from()\n\
             f.readdir()\n\
             f.readdirnames()\n\
             f.seek()\n\
             f.set_deadline()\n\
             f.set_read_deadline()\n\
             f.set_write_deadline()\n\
             f.stat()\n\
             f.sync()\n\
             f.syscall_conn()\n\
             f.truncate()\n\
             f.write()\n\
             f.write_at()\n\
             f.write_string()\n\
             f.write_to()\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        good.diagnostics
    );
}
