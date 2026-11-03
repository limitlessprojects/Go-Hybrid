//! Pin the redundant cooked-string payload allocation, not whole-parser totals.
//! The unusual payload length distinguishes copies from geometric lexer buffers.
use hy_syntax::{dump_ast, parse_file};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const PAYLOAD_LEN: usize = 10_007;
struct CountedSystem;
static TRACK: AtomicBool = AtomicBool::new(false);
static PAYLOAD_COPIES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if TRACK.load(Ordering::Relaxed) && layout.size() == PAYLOAD_LEN && !ptr.is_null() {
            PAYLOAD_COPIES.fetch_add(1, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let out = System.realloc(ptr, layout, size);
        if TRACK.load(Ordering::Relaxed) && size == PAYLOAD_LEN && !out.is_null() {
            PAYLOAD_COPIES.fetch_add(1, Ordering::Relaxed);
        }
        out
    }
}
#[global_allocator]
static ALLOCATOR: CountedSystem = CountedSystem;
#[test]
fn plain_literal_avoids_a_discarded_payload_copy() {
    let payload = "x".repeat(PAYLOAD_LEN);
    let source = format!("fn main() {{ text := \"{payload}\" }}");
    PAYLOAD_COPIES.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let parsed = parse_file("plain.hy", &source);
    TRACK.store(false, Ordering::Relaxed);
    let file = parsed.expect("plain string parses");
    assert!(
        dump_ast(&file).contains(&payload),
        "literal payload preserved"
    );
    assert_eq!(
        PAYLOAD_COPIES.load(Ordering::Relaxed),
        1,
        "one token-to-AST payload copy; no discarded interpolation copy"
    );
}
