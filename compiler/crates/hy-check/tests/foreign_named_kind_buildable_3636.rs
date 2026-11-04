use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn foreign_named_curated_types_reject_scalar_interop_3636() {
    let cases = [
        // a1: initializer mismatch
        ("var f os.file\nvar n int = f\n_ = n", "initializer"),
        // a2: assignment mismatch
        ("var r bufio.reader\nr = 1\n_ = r", "assignment"),
        // a3: string init mismatch
        ("var f os.file\nvar s string = f\n_ = s", "string init"),
        // a4: call argument mismatch
        ("var f os.file\ntake(f)", "call argument"),
        // a5: return mismatch (tested via separate fn below)
        // a6: slice literal element mismatch
        ("var f os.file\nxs := []int{f}\n_ = xs", "slice literal"),
    ];

    for (stmt, label) in cases {
        let src = format!("import \"os\"\nimport \"bufio\"\nfn take(n int) {{}}\nfn main() {{\n{stmt}\n}}\n");
        let r = check_file(&parse_file("test.hy", &src).unwrap());
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case `{label}` must reject: diagnostics={:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
            "case `{label}` must emit HYB-E1002: diagnostics={:?}",
            r.diagnostics
        );
    }

    // a5: return mismatch
    let ret_src = "import \"os\"\nfn get() int {\n    var f os.file\n    return f\n}\n";
    let r = check_file(&parse_file("ret.hy", ret_src).unwrap());
    assert_eq!(r.verdict, Verdict::Reject, "return mismatch must reject");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "return mismatch must emit HYB-E1002"
    );

    // Negative controls: legal interface satisfaction and same-type copy must accept
    let accept_src = r#"
import "os"

fn main() {
    var f1 os.file
    var f2 os.file = f1
    _ = f2

    var a any = f1
    _ = a
}
"#;
    let r = check_file(&parse_file("accept.hy", accept_src).unwrap());
    assert_eq!(r.verdict, Verdict::Accept, "legal foreign interop must accept: {:?}", r.diagnostics);
}
