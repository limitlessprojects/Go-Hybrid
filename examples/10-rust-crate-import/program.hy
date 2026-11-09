// 10-rust-crate-import — the other direction: a Hybrid program whose hot loop
// lives in a pinned Rust crate, called through the sub-5ns boundary, with the
// `?` operator unwrapping Result<T, E> into Hybrid's (T, error).
// TARGET SYNTAX. The Rust side it drives: sha3.rs (below, as a comment).

import "rust/sha3"        // pins sha3 0.10; API extracted via rustdoc JSON

fn main() {
    // strings cross as ptr+len views; digest comes back as a slice view
    // owned by the Hybrid side's arena (freed when the scope exits)
    let words = ["hybrid", "go", "rust"]
    for w in words {
        digest := sha3.digest256(w)?          // Result<T, Error> -> (T, error) via `?`
        fmt.println("{w}: {hex.encode(digest[..8])}…")
    }

    // hot loop in Rust: one boundary crossing, a million iterations inside
    t0 := time.now()
    checksum := sha3.stream_checksum(1_000_000, seed=7)?
    dt := time.since(t0)
    fmt.println("1M hashes in {dt.milliseconds}ms, checksum={checksum}")
}

// ---- the pinned Rust crate surface (rust/sha3), for reference ----
// #[no_mangle] pub extern "C" fn hybrid_sha3_digest256(data: *const u8, len: usize,
//     out: *mut u8 /*32*/) -> i32 /*0=ok, 1=err*/
// #[no_mangle] pub extern "C" fn hybrid_sha3_stream_checksum(iters: u64, seed: u64) -> u64;
