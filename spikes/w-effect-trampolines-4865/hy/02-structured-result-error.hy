// EXPECTED-VERDICT: ACCEPT
// Spike W-4865 (Proposal #4865) — P1 positive control 2:
// Structured Result<T, E> error crossing via terminal ERR tag without
// cross-boundary panic unwinding (interfaces/abi-types.md §2 line 33 & §6).

struct DecodeErr {
    code: u32
    offset: u64
}

effect ReadChunk(seq: u64, dst_cap: u32) -> (n: u32)

@[effect_session(ReadChunk)]
extern rust fn decode_frame(max_bytes: u32) -> Result[u64, DecodeErr]

fn decode_checked(src: []u8) -> (u64, error) {
    // Terminal ReturnTag::ERR carries POD DecodeErr by value in the trampoline
    // result slot; Rust RAII resources drop deterministically before returning.
    res := handle decode_frame(len(src)) with {
        ReadChunk(seq, dst_cap, scratch) => {
            n := copy_into(scratch, src, dst_cap)
            resume(n)
        }
    }
    return unwrap_or_err(res)
}
