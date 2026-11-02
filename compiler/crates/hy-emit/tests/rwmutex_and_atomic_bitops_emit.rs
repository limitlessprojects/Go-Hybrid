use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn rwmutex_and_rw_mutex_spellings_emit_go_sync_rwmutex_and_methods() {
    for field_ty in ["sync.rwmutex", "sync.rw_mutex"] {
        let src = format!(
            "struct stats {{\n\
                 rw {field_ty}\n\
                 n int\n\
             }}\n\
             fn main() {{\n\
                 var s stats\n\
                 s.rw.rlock()\n\
                 v := s.n\n\
                 s.rw.runlock()\n\
                 s.rw.try_rlock()\n\
                 fmt.println(\"v={{v}}\")\n\
             }}\n"
        );
        let ast = parse_file("rwmutex_emit.hy", &src).expect("parses");
        let go = emit_go(&ast).expect("emits");
        assert!(
            go.contains("sync.RWMutex"),
            "expected `sync.RWMutex` for `{field_ty}`, got:\n{go}"
        );
        assert!(
            !go.contains("sync.Rwmutex") && !go.contains("sync.RwMutex"),
            "must not emit invalid Go type spelling for `{field_ty}`, got:\n{go}"
        );
        assert!(
            go.contains("s.rw.RLock()")
                && go.contains("s.rw.RUnlock()")
                && go.contains("s.rw.TryRLock()"),
            "expected RLock/RUnlock/TryRLock method spellings for `{field_ty}`, got:\n{go}"
        );
    }
}

#[test]
fn atomic_integer_and_or_methods_emit_exported_go_names() {
    for atomic_ty in [
        "atomic.int32",
        "atomic.int64",
        "atomic.uint32",
        "atomic.uint64",
        "atomic.uintptr",
    ] {
        let src = format!(
            "fn main() {{\n\
                 var h {atomic_ty}\n\
                 h.store(6)\n\
                 h.and(3)\n\
                 h.or(2)\n\
                 fmt.println(\"v={{h.load()}}\")\n\
             }}\n"
        );
        let ast = parse_file("atomic_bitops.hy", &src).expect("parses");
        let go = emit_go(&ast).expect("emits");
        assert!(
            go.contains("h.And(3)") && go.contains("h.Or(2)"),
            "expected exported `h.And(3)` and `h.Or(2)` for `{atomic_ty}`, got:\n{go}"
        );
        assert!(
            !go.contains("h.and(") && !go.contains("h.or("),
            "must not emit lowercase `h.and`/`h.or` for `{atomic_ty}`, got:\n{go}"
        );
    }
}
