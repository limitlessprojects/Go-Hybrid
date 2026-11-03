//! Parse-only allocation/time reproduction; no external dependencies.
use hy_syntax::{dump_ast, parse_file};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
struct CountedSystem;
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if TRACK.load(Ordering::Relaxed) && !ptr.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let out = System.realloc(ptr, layout, size);
        if TRACK.load(Ordering::Relaxed) && !out.is_null() {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size, Ordering::Relaxed);
        }
        out
    }
}
#[global_allocator]
static ALLOCATOR: CountedSystem = CountedSystem;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert!(
        args.len() >= 3,
        "usage: parser_strings snapshot|bench FILE [ITERATIONS]"
    );
    let source = std::fs::read_to_string(&args[2]).expect("read input");
    if args[1] == "snapshot" {
        match parse_file("snapshot.hy", &source) {
            Ok(file) => println!("OK\n{}", dump_ast(&file)),
            Err(diags) => println!("ERR\n{diags:#?}"),
        }
        return;
    }
    assert_eq!(args[1], "bench");
    let iterations: usize = args
        .get(3)
        .map(|v| v.parse().expect("iterations"))
        .unwrap_or(20);
    assert!((1..=1000).contains(&iterations));
    drop(parse_file("bench.hy", &source).expect("warm parse"));
    CALLS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::Relaxed);
    let counted = parse_file("bench.hy", black_box(&source));
    TRACK.store(false, Ordering::Relaxed);
    let calls = CALLS.load(Ordering::Relaxed);
    let bytes = BYTES.load(Ordering::Relaxed);
    drop(counted.expect("counted parse"));
    let start = Instant::now();
    for _ in 0..iterations {
        drop(black_box(
            parse_file("bench.hy", black_box(&source)).expect("timed parse"),
        ));
    }
    let elapsed = start.elapsed().as_nanos();
    println!("{{\"iterations\":{iterations},\"elapsed_ns\":{elapsed},\"alloc_calls\":{calls},\"requested_bytes\":{bytes},\"source_bytes\":{}}}", source.len());
}
