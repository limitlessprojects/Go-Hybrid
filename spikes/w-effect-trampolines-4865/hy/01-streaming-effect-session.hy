// EXPECTED-VERDICT: ACCEPT
// Spike W-4865 (Proposal #4865) — P1 positive control 1:
// Non-reentrant streaming callbacks across the Go<->Rust boundary with
// single-pin session amortization (interfaces/abi-types.md §2 & §6).

effect ReadChunk(seq: u64, dst_cap: u32) -> (n: u32)
effect LogMetric(code: u32, value: u64) -> ()

@[effect_session(ReadChunk, LogMetric)]
extern rust fn stream_checksum(seed: u64, total_chunks: u32) -> u64

fn run_stream(chunks: [][]u8) -> u64 {
    // Emitted Go trampoline acquires ONE runtime.Pinner lease for `scratch`,
    // binds `defer frame.abort(gen)` + `defer pinner.Unpin()` BEFORE loop entry,
    // and dispatches yielded effects on the normal splittable Go stack.
    return handle stream_checksum(0, len(chunks)) with {
        ReadChunk(seq, dst_cap, scratch) => {
            n := copy_into(scratch, chunks[seq], dst_cap)
            resume(n)
        }
        LogMetric(code, value) => {
            record_metric(code, value)
            resume()
        }
    }
}
