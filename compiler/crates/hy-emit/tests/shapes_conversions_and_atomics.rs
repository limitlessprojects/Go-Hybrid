use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

#[test]
fn go_spelled_uint64_and_int64_conversions_fold_out_of_range_constants() {
    let go = emit(
        "fn main() {\n\
         \ta := uint64(-1)\n\
         \tb := int64(9223372036854775808)\n\
         \tc := uint64(42)\n\
         \tfmt.println(a, b, c)\n\
         }\n",
    );
    assert!(
        go.contains("uint64(18446744073709551615)"),
        "expected uint64(-1) to fold via SPEC X4, got:\n{}",
        go
    );
    assert!(
        go.contains("int64(-9223372036854775808)"),
        "expected int64(2^63) to fold via SPEC X4, got:\n{}",
        go
    );
    assert!(
        go.contains("uint64(42)"),
        "expected fitting uint64(42) to remain unchanged, got:\n{}",
        go
    );
}

#[test]
fn scalar_atomics_and_compare_and_swap_on_locals_and_struct_fields_emit_go_methods() {
    let go = emit(
        "struct worker_state {\n\
         \tready atomic.bool\n\
         \tcount32 atomic.int32\n\
         \tcount64 atomic.int64\n\
         \tu32c atomic.uint32\n\
         \tu64c atomic.uint64\n\
         \tptr atomic.uintptr\n\
         \trw sync.rwmutex\n\
         }\n\
         \n\
         fn main() {\n\
         \tvar hits atomic.int64\n\
         \thits.compare_and_swap(0, 1)\n\
         \ts := worker_state{}\n\
         \ts.ready.store(true)\n\
         \ts.ready.compare_and_swap(true, false)\n\
         \ts.count32.add(1)\n\
         \ts.count32.compare_and_swap(1, 2)\n\
         \ts.count64.compare_and_swap(0, 5)\n\
         \ts.u32c.add(1)\n\
         \ts.u64c.swap(9)\n\
         \ts.ptr.store(0)\n\
         \ts.rw.try_lock()\n\
         \ts.rw.try_rlock()\n\
         \tfmt.println(hits.load(), s.ready.load(), s.count32.load())\n\
         }\n",
    );
    assert!(
        go.contains("hits.CompareAndSwap(0, 1)"),
        "expected hits.CompareAndSwap, got:\n{}",
        go
    );
    assert!(
        go.contains("s.ready.Store(true)") && go.contains("s.ready.CompareAndSwap(true, false)"),
        "expected atomic.bool methods on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("s.count32.Add(1)") && go.contains("s.count32.CompareAndSwap(1, 2)"),
        "expected atomic.int32 methods on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("s.count64.CompareAndSwap(0, 5)"),
        "expected atomic.int64 CompareAndSwap on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("s.u32c.Add(1)")
            && go.contains("s.u64c.Swap(9)")
            && go.contains("s.ptr.Store(0)"),
        "expected atomic.uint32/uint64/uintptr methods on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("s.rw.TryLock()") && go.contains("s.rw.TryRLock()"),
        "expected sync.rwmutex TryLock/TryRLock on struct field, got:\n{}",
        go
    );
}

#[test]
fn url_and_filepath_roots_emit_go_stdlib_import_paths() {
    let go = emit(
        "fn check_url(v url.values) {\n\
         \tfmt.println(v.encode())\n\
         }\n\
         \n\
         fn main() {\n\
         \tvar v url.values\n\
         \tcheck_url(v)\n\
         }\n",
    );
    assert!(
        go.contains("\"net/url\""),
        "expected url.values to import net/url, got:\n{}",
        go
    );
}
