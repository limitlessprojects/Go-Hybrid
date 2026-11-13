//! Regression tests for #4399: uncalled `Expr::FnLit` closures must not launder
//! `Closed`/`Drained`/`Moved` obligations onto enclosing SPSC pipe scopes.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_reject(src: &str, expected_codes: &[&str]) {
    let res = check_file(&parse_file("test_4399.hy", src).expect("snippet must parse"));
    assert_eq!(res.verdict, Verdict::Reject, "expected Reject, got Accept");
    for code in expected_codes {
        assert!(res.diagnostics.iter().any(|d| d.code == *code), "missing {code}: {:?}", res.diagnostics);
    }
}

#[test]
fn uncalled_fnlit_close_and_drain_do_not_discharge_outer_pipe() {
    assert_reject("fn main() {\n    tx, rx := hy.pipe[int]()\n    _ = fn() {\n        tx.close()\n        rx.drain()\n    }\n}\n", &["HYB-E4004", "HYB-E4005"]);
}

#[test]
fn uncalled_fnlit_for_iterate_drain_does_not_discharge_e4005() {
    assert_reject("fn main() {\n    tx, rx := hy.pipe[int]()\n    tx.close()\n    f := fn() {\n        for v in rx {\n            _ = v\n        }\n    }\n    _ = f\n}\n", &["HYB-E4005"]);
}

#[test]
fn uncalled_fnlit_moves_do_not_discharge_e4004_or_e4005() {
    assert_reject("fn worker(tx hy.send_cap[int]) { tx.close() }\nfn consumer(rx hy.recv_cap[int]) { rx.drain() }\nfn main() {\n    managed(\"hatch\", \"mutex\") {\n        tx, rx := hy.pipe[int]()\n        _ = fn() {\n            task worker(move tx)\n            task consumer(move rx)\n        }\n    }\n}\n", &["HYB-E4004", "HYB-E4005"]);
}

#[test]
fn stored_closure_called_twice_or_in_loop_rejects_double_move_e3001() {
    assert_reject(
        "fn worker(tx hy.send_cap[int]) {\n    tx.send(1)\n    tx.close()\n}\nfn main() {\n    managed(\"hatch\", \"mutex\") {\n        tx, rx := hy.pipe[int]()\n        spawn := fn() {\n            task worker(move tx)\n        }\n        spawn()\n        spawn()\n        rx.drain()\n    }\n}\n",
        &["HYB-E3001"],
    );
    assert_reject(
        "fn worker(tx hy.send_cap[int]) { tx.close() }\nfn main() {\n    managed(\"hatch\", \"mutex\") {\n        tx, rx := hy.pipe[int]()\n        spawn := fn() {\n            task worker(move tx)\n        }\n        for _ in 0..2 {\n            spawn()\n        }\n        rx.drain()\n    }\n}\n",
        &["HYB-E3001"],
    );
}

#[test]
fn nested_iife_inside_uncalled_fnlit_still_rejects() {
    assert_reject(
        "fn main() {\n    tx, rx := hy.pipe[int]()\n    _ = fn() {\n        (fn() {\n            tx.close()\n            rx.drain()\n        })()\n    }\n}\n",
        &["HYB-E4004", "HYB-E4005"],
    );
}

#[test]
fn iife_and_defer_iife_and_closure_local_pipe_accept() {
    let src = "fn worker(tx hy.send_cap[int]) {\n    tx.send(7)\n    tx.close()\n}\nfn main() {\n    tx1, rx1 := hy.pipe[int]()\n    (fn() {\n        tx1.send(1)\n        tx1.close()\n    })()\n    _ = rx1.recv()\n    rx1.drain()\n    tx2, rx2 := hy.pipe[int]()\n    defer (fn() {\n        tx2.close()\n        rx2.drain()\n    })()\n    _ = fn() {\n        inner_tx, inner_rx := hy.pipe[int]()\n        task worker(move inner_tx)\n        inner_rx.drain()\n    }\n}\n";
    let f = parse_file("test_4399_ok.hy", src).expect("snippet must parse");
    let res = check_file(&f);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept: {:?}", res.diagnostics);
}
