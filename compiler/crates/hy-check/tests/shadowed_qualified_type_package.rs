use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn qualified_type_shadowed_by_local_package_name_rejects_e1002() {
    for src in [
        "import \"sync\"\nfn main() { sync := 1\n var m sync.Mutex\n _ = sync\n _ = m }",
        "import \"os\"\nfn f(os int) { var file os.File\n _ = os\n _ = file }\nfn main() { f(1) }",
        "import \"net\"\nfn main() { net := \"local\"\n var ip net.IP\n _ = net\n _ = ip }",
        "import \"sync\"\nfn main() { int := 1\n var p sync.Pool[int]\n _ = int\n _ = p }",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}

#[test]
fn qualified_type_with_unshadowed_package_and_local_member_name_accepts() {
    for src in [
        "import \"sync\"\nfn main() { mutex := 1\n var m sync.Mutex\n _ = mutex\n _ = m }",
        "import \"os\"\nfn main() { file := 1\n var f os.File\n _ = file\n _ = f }",
        "fn main() { int := 1\n var y int = 2\n _ = int\n _ = y }",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        // The first two must not reject mutex/file shadowing sync.Mutex/os.File
        // (the third one is an unqualified int shadow which must reject)
        if src.contains("sync.Mutex") || src.contains("os.File") {
            assert!(
                !d.iter().any(|x| x.code == "HYB-E1002" && x.message.contains("sync.Mutex")),
                "{src}: unexpected diagnostic: {d:?}"
            );
        } else {
            assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
        }
    }
}
