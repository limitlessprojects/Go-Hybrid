//! Observed allocation requests, not timing or an optimiser guarantee.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    error::Error,
    fs,
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

struct CountingSystem;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED_BYTES: AtomicUsize = AtomicUsize::new(0);

// This example only forwards System's allocation contract and records atomic
// counters. The allocator never formats output or allocates recursively.
unsafe impl GlobalAlloc for CountingSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCS.fetch_add(1, Relaxed);
            REQUESTED_BYTES.fetch_add(layout.size(), Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCS.fetch_add(1, Relaxed);
            REQUESTED_BYTES.fetch_add(layout.size(), Relaxed);
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, size) };
        if !result.is_null() {
            REALLOCS.fetch_add(1, Relaxed);
            REQUESTED_BYTES.fetch_add(size, Relaxed);
        }
        result
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: CountingSystem = CountingSystem;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let file = args
        .next()
        .ok_or("use: lexer_allocations FILE [ITERATIONS]")?;
    let iterations: usize = args.next().unwrap_or_else(|| "10".into()).parse()?;
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let source = fs::read_to_string(file)?;
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    REQUESTED_BYTES.store(0, Relaxed);
    let mut token_count = 0usize;
    for _ in 0..iterations {
        let tokens = hy_syntax::lex("input.hy", black_box(&source))
            .map_err(|d| format!("lex failed: {d:?}"))?;
        token_count += tokens.len();
        drop(black_box(tokens));
    }
    let allocs = ALLOCS.load(Relaxed);
    let reallocs = REALLOCS.load(Relaxed);
    let requested_bytes = REQUESTED_BYTES.load(Relaxed);
    println!(
        "{{\"input_bytes\":{},\"iterations\":{},\"tokens_per_iteration\":{},\"allocs\":{},\"reallocs\":{},\"requested_bytes\":{}}}",
        source.len(), iterations, token_count / iterations, allocs, reallocs, requested_bytes,
    );
    Ok(())
}
