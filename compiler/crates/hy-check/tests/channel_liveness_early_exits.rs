use hy_check::check_file;
use hy_syntax::parse_file;

fn check_codes(s: &str, want: &[&str]) {
    let got: Vec<_> = check_file(&parse_file("t.hy", s).unwrap()).diagnostics.into_iter().map(|d| d.code).collect();
    if want.is_empty() {
        assert!(got.is_empty(), "unexpected diagnostics: {got:?}");
    } else {
        for w in want { assert!(got.iter().any(|c| c == w), "missing {w} in {got:?}"); }
    }
}

#[test]
fn early_exits_and_loop_liveness() {
    let leak = &["HYB-E4004", "HYB-E4005"];
    check_codes("fn f(c bool) {\ntx, rx := hy.pipe[int]()\nif c { return }\ntx.close()\nrx.drain()\n}", leak);
    check_codes("fn s() (int, error) { return 0, nil }\nfn f() error {\ntx, rx := hy.pipe[int]()\n_ = s()?\ntx.close()\nrx.drain()\nreturn nil\n}", leak);
    check_codes("fn f(c bool) {\ntx, rx := hy.pipe[int]()\nif c {\ntx.close()\nrx.drain()\nreturn\n}\n}", leak);
    check_codes("fn f(c bool) {\ntx, rx := hy.pipe[int]()\nif c {\ntx.close()\nrx.drain()\nreturn\n}\ntx.close()\nrx.drain()\n}", &[]);
    check_codes("fn f(c bool) {\ntx, rx := hy.pipe[int]()\nif c {\ntx.close()\nrx.drain()\nreturn\n} else {\ntx.close()\nrx.drain()\nreturn\n}\n}", &[]);
    check_codes("fn f() {\nfor i in 0..3 {\ntx, rx := hy.pipe[int]()\ntx.close()\nrx.drain()\n}\n}", &[]);
    check_codes("fn f(c bool) {\nfor i in 0..3 {\ntx, rx := hy.pipe[int]()\nif c { break }\ntx.close()\nrx.drain()\n}\n}", leak);
    check_codes("fn f(c bool) {\nfor i in 0..3 {\ntx, rx := hy.pipe[int]()\nif c { continue }\ntx.close()\nrx.drain()\n}\n}", leak);
    check_codes("fn w(tx hy.send_cap[int]) { tx.close() }\nfn f() {\ntx, rx := hy.pipe[int]()\nfor i in 0..3 { task w(move tx) }\nrx.drain()\n}", &["HYB-E3001"]);
    check_codes("fn w(tx hy.send_cap[int]) { tx.close() }\nfn f() {\ntx, rx := hy.pipe[int]()\nfor i in 0..3 {\ntask w(move tx)\ncontinue\n}\nrx.drain()\n}", &["HYB-E3001"]);
    check_codes("fn f(c bool) {\ntx, rx := hy.pipe[int]()\nfor i in 0..3 {\nif c { break } else { continue }\n}\n}", leak);
    check_codes("fn f(c bool) {\ntx, rx := hy.pipe[int]()\nfor i in 0..3 {\nif c { break } else {\ntx.close()\nrx.drain()\nreturn\n}\n}\n}", leak);
    check_codes("fn f() {\nfor i in 0..3 {\ntx, rx := hy.pipe[int]()\nselect {\ndefault:\nbreak\n}\ntx.close()\nrx.drain()\n}\n}", &[]);
    check_codes("fn f() {\nfor i in 0..3 {\ntx, rx := hy.pipe[int]()\nselect {\ndefault:\nbreak loop\n}\ntx.close()\nrx.drain()\n}\n}", leak);
}
