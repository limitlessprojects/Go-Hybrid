//! Regression tests for interprocedural parameter-store tracking across:
//! 1. Auto-addressed borrowable composite arguments and pointer receivers (`attach(h, b)`, `b.attach_to(h)`).
//! 2. Closure and IIFE captures of outer borrowable `[N]T` / `struct` bindings (`put := fn(dst *holder) { dst.s = a[:] }`).
//! 3. File-level and block-scoped `const` `[N]T` / `struct` composites sliced into a destination parameter (`HYB-E3006` / `HYB-E3004`).
//! 4. Pointer type aliases (`type buf_ptr = *buf`) in borrow-returning and parameter-storing calls.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty() && r.verdict == Verdict::Accept,
        "must accept, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
}

#[test]
fn rejects_autoaddr_arg_and_receiver_param_store_post_move_mutation() {
    rejects_with(
        "struct holder {\n\
            s []string\n\
        }\n\
        struct buf {\n\
            data [4]string\n\
        }\n\
        fn attach(h *holder, b *buf) {\n\
            h.s = b.data[:]\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            var b buf\n\
            attach(h, b)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            b.data[0] = \"mutated\"\n\
            <-done\n\
        }\n",
        "HYB-E3003",
    );

    rejects_with(
        "struct holder {\n\
            s []int\n\
        }\n\
        struct buf {\n\
            data [4]int\n\
        }\n\
        fn (b *buf) attach_to(h *holder) {\n\
            h.s = b.data[:]\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            var b buf\n\
            b.attach_to(h)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            b.data[0] = 7\n\
            <-done\n\
        }\n",
        "HYB-E3003",
    );
}

#[test]
fn rejects_closure_and_iife_captured_array_or_struct_param_store() {
    rejects_with(
        "struct holder {\n\
            s []int\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var a [4]int\n\
            var h holder\n\
            put := fn(dst *holder) {\n\
                dst.s = a[:]\n\
            }\n\
            put(h)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            a[0] = 9\n\
            <-done\n\
        }\n",
        "HYB-E3003",
    );

    rejects_with(
        "struct holder {\n\
            s []int\n\
        }\n\
        struct buf {\n\
            data [4]int\n\
        }\n\
        fn attach(h *holder, b *buf) {\n\
            h.s = b.data[:]\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var b buf\n\
            var h holder\n\
            put := fn(dst *holder) {\n\
                attach(dst, b)\n\
            }\n\
            put(h)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            b.data[0] = 9\n\
            <-done\n\
        }\n",
        "HYB-E3003",
    );
}

#[test]
fn rejects_const_composite_sliced_into_param_write_and_move() {
    // T22 deep immutability: mutating through slice stored from file-level const composite -> HYB-E3006.
    rejects_with(
        "struct holder {\n\
            s []int\n\
        }\n\
        struct buf {\n\
            data [4]int\n\
        }\n\
        const global_buf = buf{data: [4]int{10, 20, 30, 40}}\n\
        fn leak_const(h *holder) {\n\
            h.s = global_buf.data[:]\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            leak_const(h)\n\
            h.s[0] = 999\n\
        }\n",
        "HYB-E3006",
    );

    // O5.1 / O5.4: moving a holder aliasing file-level const composite across a task boundary -> HYB-E3004.
    rejects_with(
        "struct holder {\n\
            s []int\n\
        }\n\
        struct buf {\n\
            data [4]int\n\
        }\n\
        const global_buf = buf{data: [4]int{10, 20, 30, 40}}\n\
        fn attach(h *holder, b *buf) {\n\
            h.s = b.data[:]\n\
        }\n\
        fn leak_const_indirect(h *holder) {\n\
            attach(h, global_buf)\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            leak_const_indirect(h)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            <-done\n\
        }\n",
        "HYB-E3004",
    );
}

#[test]
fn rejects_pointer_type_alias_autoaddr_borrow_and_param_store() {
    rejects_with(
        "struct buf {\n\
            data [4]int\n\
        }\n\
        type buf_ptr = *buf\n\
        fn view(b buf_ptr) []int {\n\
            return b.data[:]\n\
        }\n\
        fn worker(s []int, done chan int) {\n\
            _ = s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var b buf\n\
            s := view(b)\n\
            done := make(chan int, 1)\n\
            task worker(move s, done)\n\
            b.data[0] = 1\n\
            <-done\n\
        }\n",
        "HYB-E3003",
    );

    rejects_with(
        "struct holder {\n\
            s []int\n\
        }\n\
        struct buf {\n\
            data [4]int\n\
        }\n\
        type buf_ptr = *buf\n\
        fn attach_alias(h *holder, b buf_ptr) {\n\
            h.s = b.data[:]\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            var b buf\n\
            var cb fn(h *holder, b buf_ptr) = attach_alias\n\
            cb(h, b)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            b.data[0] = 1\n\
            <-done\n\
        }\n",
        "HYB-E3003",
    );
}

#[test]
fn accepts_owned_move_without_post_move_use_and_by_value_copy() {
    // Moving `h` is sound when `b` is never accessed after `move h`.
    accepts(
        "struct holder {\n\
            s []string\n\
        }\n\
        struct buf {\n\
            data [4]string\n\
        }\n\
        fn attach(h *holder, b *buf) {\n\
            h.s = b.data[:]\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            var b buf\n\
            b.data[0] = \"ok\"\n\
            attach(h, b)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            <-done\n\
        }\n",
    );

    // Passing `b` BY VALUE (`b buf`) allocates an independent copy inside `copy_into`.
    accepts(
        "struct holder {\n\
            s []string\n\
        }\n\
        struct buf {\n\
            data [4]string\n\
        }\n\
        fn copy_into(h *holder, b buf) {\n\
            h.s = b.data[:]\n\
        }\n\
        fn worker(h holder, done chan int) {\n\
            _ = h.s[0]\n\
            done <- 1\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            var b buf\n\
            copy_into(h, b)\n\
            done := make(chan int, 1)\n\
            task worker(move h, done)\n\
            b.data[0] = \"independent\"\n\
            <-done\n\
        }\n",
    );

    // Read-only view of a file-level const composite remains valid when neither mutated nor moved.
    accepts(
        "import \"fmt\"\n\
        struct holder {\n\
            s []int\n\
        }\n\
        struct buf {\n\
            data [4]int\n\
        }\n\
        const global_buf = buf{data: [4]int{10, 20, 30, 40}}\n\
        fn leak_const(h *holder) {\n\
            h.s = global_buf.data[:]\n\
        }\n\
        fn main() {\n\
            var h holder\n\
            leak_const(h)\n\
            fmt.println(h.s[0])\n\
        }\n",
    );
}
