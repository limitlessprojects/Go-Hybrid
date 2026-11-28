// EXPECTED-VERDICT: REJECT EFFECT-BOUNDARY
// Spike W-4865 (Proposal #4865) — P1 negative control exercising all five
// static effect-boundary rejection rules (HYB-E3801..HYB-E3805).

effect ReadChunk(seq: u64, dst_cap: u32) -> (n: u32)
effect LogMetric(code: u32, value: u64) -> ()

// HYB-E3805: non-POD / GC-pointer payload in effect ABI signature
effect BadPayload(m: map[string]int) -> ()

@[effect_session(ReadChunk, LogMetric)]
extern rust fn stream_checksum(seed: u64, total_chunks: u32) -> u64

fn bad_unhandled_effect() -> u64 {
    // HYB-E3801: declared LogMetric effect is missing a handler arm
    return handle stream_checksum(0, 4) with {
        ReadChunk(seq, dst_cap, scratch) => {
            resume(0)
        }
    }
}

fn bad_reentrant_handler() -> u64 {
    // HYB-E3802: handler re-enters an effect session synchronously while active
    return handle stream_checksum(0, 4) with {
        ReadChunk(seq, dst_cap, scratch) => {
            _ := stream_checksum(1, 1)
            resume(0)
        }
        LogMetric(code, value) => {
            resume()
        }
    }
}

fn bad_scratch_escape() -> []u8 {
    // HYB-E3803: pinned scratch slice view escapes handler scope
    var leaked []u8
    _ := handle stream_checksum(0, 4) with {
        ReadChunk(seq, dst_cap, scratch) => {
            leaked = scratch
            resume(0)
        }
        LogMetric(code, value) => {
            resume()
        }
    }
    return leaked
}

fn bad_session_escape_and_reuse(sess: EffectSession) {
    // HYB-E3804: linear EffectSession crosses task boundary / double-resumed
    task worker(move sess)
    sess.resume(0)
}
