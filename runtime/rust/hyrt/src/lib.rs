//! hyrt — the Hybrid runtime bridge (Phase 2, Batch F).
//!
//! One staticlib per binary (abi-types.md §4 rule 6). Every export is
//! `#[no_mangle] extern "C"`, takes only C-ABI words (u64/uintptr/pointers),
//! never allocates Go-visible memory, never calls back into Go, and cannot
//! unwind (panic=abort in Cargo.toml — E9: panics abort at the Rust edge,
//! they are never errors).
//!
//! Error contract (Spike 5): every fallible export returns an i32 status —
//! 0 = ok, 1 = mapped error — and writes its real result through an
//! out-pointer the GO side owns. The Go binding maps status 1 to a Hybrid
//! `error` (sentinel table, wrap-on-demand: ≤1 allocation, E5's budget).
//! No Rust string ever crosses back; messages are Go-side constants.
//!
//! Wrappers are THIN: the pinned crates do the work. API extraction is the
//! descriptor work (rustdoc-JSON automation is post-MVP; the hand-curated
//! surfaces here are exactly what the pinned target programs need, grown by
//! Decision Log entries — SPEC M5/M6 discipline applies to crates too).

// Every export takes C-ABI raw pointers by definition (abi-types.md §5's
// pinned-borrow + generation-tag protocol is the safety contract; the Go
// broker is its only caller, and each export null-guards its own args).
// The lint would demand `unsafe fn` on all 18 — it changes nothing for the
// C caller and adds nothing the in-fn guards don't already enforce.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ptr;

// ---------------------------------------------------------------------------
// rust/sha3 (pinned 0.10.8) — example 10's hot-loop crate
// ---------------------------------------------------------------------------

/// `sha3.digest256(data)` — SHA3-256 of `data[0..len]`, 32 bytes into `out`.
/// Status 0 ok / 1 err (out or data null, or len with null data).
#[no_mangle]
pub extern "C" fn hy_sha3_digest256(
    data: *const u8,
    len: usize,
    out: *mut u8, /* 32 */
) -> i32 {
    if out.is_null() || len > isize::MAX as usize || (len > 0 && data.is_null()) {
        return 1;
    }
    use sha3::{Digest, Sha3_256};
    // Empty Go slices (`[]byte{}` / `nil`) cross with `data == null, len == 0`;
    // `slice::from_raw_parts` requires a non-null pointer even when `len == 0`.
    let slice: &[u8] = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, len) }
    };
    let digest = Sha3_256::digest(slice);
    unsafe {
        ptr::copy_nonoverlapping(digest.as_ptr(), out, 32);
    }
    0
}

/// `sha3.stream_checksum(iters, seed)` — the hot-loop shape: ONE boundary
/// crossing, `iters` SHA3-256 permutations inside Rust, folding each digest's
/// first word into a u64 checksum. Deterministic per pinned crate version
/// (example 10's expected output pins the value).
#[no_mangle]
pub extern "C" fn hy_sha3_stream_checksum(iters: u64, seed: u64) -> u64 {
    use sha3::{Digest, Sha3_256};
    let mut state: u64 = seed ^ 0x68796272_69645f72; // "hybrid_r"-ish tag
    let mut buf = [0u8; 8];
    for i in 0..iters {
        buf.copy_from_slice(&state.to_le_bytes());
        let d = Sha3_256::digest(buf); // per-iteration: absorb+squeeze 8 bytes
        let word = u64::from_le_bytes(d[0..8].try_into().unwrap());
        state = state.rotate_left(13) ^ word ^ i;
    }
    state
}

/// The G2 overhead bench's no-op (Batch J): the SHIPPING crossing —
/// hybridge.Cross1's fast trampoline + the C ABI — with zero payload work.
/// Returns the argument (one register move); the measured cost is the
/// boundary itself, exactly the quantity SPEC §6's ≤5 ns budget names.
#[no_mangle]
pub extern "C" fn hy_nop(x: u64) -> u64 {
    x
}

/// Win64 ABI regression probe (fleet-found misalignment, 2026-10-14): the
/// address of a 16-byte-aligned local, mod 16. A conforming Win64 crossing
/// (RSP % 16 == 0 at the CALL, RSP % 16 == 8 at callee entry) leaves the
/// prologue's frame 16-aligned, so this returns 0. The misaligned-8 fast
/// path returned 8 and crashed the first `movaps` in any gcc-built callee
/// with aligned SSE stores (sha3's frame zeroing). Pure compute — SMALL
/// class; Cross1S exercises the exact fast trampoline shape.
#[no_mangle]
pub extern "C" fn hyrt_stack_alignment_probe() -> u64 {
    #[repr(align(16))]
    struct Align16([u8; 16]);
    let a = Align16([0; 16]);
    let p = &a as *const Align16 as u64;
    std::hint::black_box(&a);
    p & 15
}

// ---------------------------------------------------------------------------
// rust/serde_json (pinned 1.0.140) — example 05's transform crate
// ---------------------------------------------------------------------------
//
// JSON values cross as RUST-OWNED HANDLES (abi-types §2): a u64 carrying
// (generation << 32) | (index + 1) into hyrt's owning-side table. Go only
// ever sees integers; release is idempotent; a stale generation reads
// INVALID. The table's storage is a Mutex<Vec<Option<Box<Value>>>> — MVP
// scale (example 05 touches ~10 values), contention irrelevant.

use serde_json::{Map, Value as Json};
use std::sync::Mutex;

struct Slot {
    gen: u32,
    val: Option<Json>,
}

struct Table {
    slots: Vec<Slot>, // generation-tagged slots; val == None when free
    free: Vec<usize>,
}

static TABLE: Mutex<Table> = Mutex::new(Table {
    slots: Vec::new(),
    free: Vec::new(),
});

const ERR_TABLE: i32 = 1; // table poisoned / slot invalid
const ERR_JSON: i32 = 2; // the JSON operation itself failed

fn next_gen(g: u32) -> u32 {
    // Wrap before `u32::MAX` (`0xFFFF_FFFF`) so `handle_pack(0xFFFF_FFFE, gen)`
    // can never produce the reserved `u64::MAX` invalid-handle sentinel.
    if g >= u32::MAX - 1 {
        1
    } else {
        g + 1
    }
}

fn handle_pack(idx: usize, gen: u32) -> u64 {
    debug_assert!(idx < (u32::MAX - 1) as usize && gen > 0 && gen < u32::MAX);
    ((gen as u64) << 32) | (idx as u64 + 1)
}

/// (index, generation) for a live handle, or None (stale/unknown).
fn handle_unpack(h: u64) -> Option<(usize, u32)> {
    if h == 0 || h == u64::MAX {
        return None;
    }
    let raw_idx = (h & 0xFFFF_FFFF) as usize;
    let gen = (h >> 32) as u32;
    if raw_idx == 0 || gen == 0 {
        return None;
    }
    Some((raw_idx - 1, gen))
}

fn with_table(f: impl FnOnce(&mut Table) -> Result<(), i32>) -> i32 {
    match TABLE.lock() {
        Ok(mut t) => match f(&mut t) {
            Ok(()) => 0,
            Err(c) => c,
        },
        Err(_) => ERR_TABLE,
    }
}

fn table_insert(t: &mut Table, v: Json) -> u64 {
    if let Some(idx) = t.free.pop() {
        let gen = t.slots[idx].gen;
        t.slots[idx].val = Some(v);
        return handle_pack(idx, gen);
    }
    let gen = 1u32;
    t.slots.push(Slot { gen, val: Some(v) });
    handle_pack(t.slots.len() - 1, gen)
}

fn table_get(t: &Table, h: u64) -> Result<&Json, i32> {
    let (idx, gen) = handle_unpack(h).ok_or(ERR_TABLE)?;
    let slot = t.slots.get(idx).ok_or(ERR_TABLE)?;
    if slot.gen != gen {
        return Err(ERR_TABLE); // stale generation: INVALID, never corrupt
    }
    slot.val.as_ref().ok_or(ERR_TABLE)
}

fn table_get_mut(t: &mut Table, h: u64) -> Result<&mut Json, i32> {
    let (idx, gen) = handle_unpack(h).ok_or(ERR_TABLE)?;
    let slot = t.slots.get_mut(idx).ok_or(ERR_TABLE)?;
    if slot.gen != gen {
        return Err(ERR_TABLE);
    }
    slot.val.as_mut().ok_or(ERR_TABLE)
}

/// A C-ABI string view: (ptr, len) — Go pins the backing bytes for the call.
#[repr(C)]
pub struct HyStr {
    ptr: *const u8,
    len: usize,
}

impl HyStr {
    fn as_str(&self) -> Result<&'static str, i32> {
        if self.len == 0 {
            return Ok("");
        }
        if self.ptr.is_null() || self.len > isize::MAX as usize {
            return Err(ERR_TABLE);
        }
        let bytes = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
        std::str::from_utf8(bytes).map_err(|_| ERR_JSON)
    }

    /// Win64 ABI discipline (PLAN row 17, found by the fleet windows leg):
    /// a 16-byte `repr(C)` struct passed BY VALUE lands in two registers
    /// under SysV (ptr+len — which the flat C shims matched by luck) but is
    /// passed BY REFERENCE on Windows — garbage downstream. Every export
    /// therefore takes strings as TWO u64s (ptr, len); the struct stays
    /// internal. Two integer u64s register-pass identically on both ABIs.
    ///
    /// SAFETY: (ptr, len) must be a valid byte range for 'static (the
    /// pinned-borrow protocol pins the Go side across the crossing).
    unsafe fn from_raw(ptr: u64, len: u64) -> Self {
        HyStr {
            ptr: ptr as *const u8,
            len: usize::try_from(len).unwrap_or(usize::MAX),
        }
    }
}

/// `serde_json.parse(text)` → handle. (example 05: `serde_json.parse(raw)?`)
#[no_mangle]
pub extern "C" fn hy_json_parse(ptr: u64, len: u64, out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    let s = match unsafe { HyStr::from_raw(ptr, len) }.as_str() {
        Ok(s) => s,
        Err(e) => return e,
    };
    let v: Json = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(_) => return ERR_JSON,
    };
    with_table(|t| {
        let h = table_insert(t, v);
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

/// `serde_json.new_object()` → handle.
#[no_mangle]
pub extern "C" fn hy_json_new_object(out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let h = table_insert(t, Json::Object(Map::new()));
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

/// `serde_json.new_array()` → handle.
#[no_mangle]
pub extern "C" fn hy_json_new_array(out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let h = table_insert(t, Json::Array(Vec::new()));
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

/// `v.array(key)` → handle of the array at `v[key]` (must BE an array).
#[no_mangle]
pub extern "C" fn hy_json_get_array(v: u64, kptr: u64, klen: u64, out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let obj = table_get(t, v)?;
        let k = unsafe { HyStr::from_raw(kptr, klen) }.as_str()?;
        let arr = obj
            .get(k)
            .ok_or(ERR_JSON)?
            .as_array()
            .ok_or(ERR_JSON)?
            .clone();
        let h = table_insert(t, Json::Array(arr));
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

/// `v.int(key)` → i64 at `v[key]`.
#[no_mangle]
pub extern "C" fn hy_json_get_int(v: u64, kptr: u64, klen: u64, out: *mut i64) -> i32 {
    if out.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let obj = table_get(t, v)?;
        let k = unsafe { HyStr::from_raw(kptr, klen) }.as_str()?;
        let n = obj.get(k).ok_or(ERR_JSON)?.as_i64().ok_or(ERR_JSON)?;
        unsafe {
            *out = n;
        }
        Ok(())
    })
}

/// `v.string(key)` → handle of a NEW string value (owned, released by Go).
/// (The binding's `String(key)` returns a Go string — copied out via
/// `hy_json_value_text`, one pinned-borrow crossing.)
#[no_mangle]
pub extern "C" fn hy_json_get_string(v: u64, kptr: u64, klen: u64, out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let obj = table_get(t, v)?;
        let k = unsafe { HyStr::from_raw(kptr, klen) }.as_str()?;
        let s = obj
            .get(k)
            .ok_or(ERR_JSON)?
            .as_str()
            .ok_or(ERR_JSON)?
            .to_string();
        let h = table_insert(t, Json::String(s));
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

/// `arr.len()` → element count.
#[no_mangle]
pub extern "C" fn hy_json_array_len(arr: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let n = table_get(t, arr)?.as_array().ok_or(ERR_JSON)?.len();
        unsafe {
            *out = n as u64;
        }
        Ok(())
    })
}

/// `arr[i]` → handle of element i.
#[no_mangle]
pub extern "C" fn hy_json_array_get(arr: u64, i: u64, out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let idx = usize::try_from(i).map_err(|_| ERR_JSON)?;
        let elem = table_get(t, arr)?
            .as_array()
            .ok_or(ERR_JSON)?
            .get(idx)
            .ok_or(ERR_JSON)?
            .clone();
        let h = table_insert(t, elem);
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

/// `arr.push_string(s)`.
#[no_mangle]
pub extern "C" fn hy_json_array_push_string(arr: u64, sptr: u64, slen: u64) -> i32 {
    with_table(|t| {
        let a = table_get_mut(t, arr)?.as_array_mut().ok_or(ERR_JSON)?;
        let sv = unsafe { HyStr::from_raw(sptr, slen) }.as_str()?.to_string();
        a.push(Json::String(sv));
        Ok(())
    })
}

/// `v.set(key, value_handle)` — moves the value handle's payload into `v`.
#[no_mangle]
pub extern "C" fn hy_json_set(v: u64, kptr: u64, klen: u64, value: u64) -> i32 {
    with_table(|t| {
        // Validate both `value` (live handle) and `v` (distinct live JSON Object)
        // before removing `value` from the table so a failed `Set` never consumes `value`.
        let (vidx, vgen) = handle_unpack(value).ok_or(ERR_TABLE)?;
        let (tidx, tgen) = handle_unpack(v).ok_or(ERR_TABLE)?;
        if vidx == tidx {
            return Err(ERR_TABLE);
        }
        {
            let vslot = t.slots.get(vidx).ok_or(ERR_TABLE)?;
            if vslot.val.is_none() || vslot.gen != vgen {
                return Err(ERR_TABLE);
            }
            let tslot = t.slots.get(tidx).ok_or(ERR_TABLE)?;
            if tslot.gen != tgen {
                return Err(ERR_TABLE);
            }
            let tobj = tslot.val.as_ref().ok_or(ERR_TABLE)?;
            if !tobj.is_object() {
                return Err(ERR_JSON);
            }
        }
        let k = unsafe { HyStr::from_raw(kptr, klen) }.as_str()?.to_string();
        let val = t.slots[vidx].val.take().ok_or(ERR_TABLE)?;
        t.slots[vidx].gen = next_gen(t.slots[vidx].gen);
        t.free.push(vidx);
        let obj = table_get_mut(t, v)?.as_object_mut().ok_or(ERR_JSON)?;
        obj.insert(k, val);
        Ok(())
    })
}

/// Write a value's JSON text (compact) into a caller buffer; `out_len` gets
/// the needed length when the buffer is too small (status 3 = try again).
#[no_mangle]
pub extern "C" fn hy_json_value_text(v: u64, out: *mut u8, cap: usize, out_len: *mut u64) -> i32 {
    if out_len.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let text = table_get(t, v)?.to_string();
        let need = text.len();
        unsafe {
            *out_len = need as u64;
        }
        if out.is_null() || cap < need {
            return Err(3); // TRY_AGAIN with the reported length
        }
        unsafe {
            ptr::copy_nonoverlapping(text.as_ptr(), out, need);
        }
        Ok(())
    })
}

/// `serde_json.to_string_pretty(v)` — same buffer protocol, pretty form.
#[no_mangle]
pub extern "C" fn hy_json_to_string_pretty(
    v: u64,
    out: *mut u8,
    cap: usize,
    out_len: *mut u64,
) -> i32 {
    if out_len.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let text = serde_json::to_string_pretty(table_get(t, v)?).map_err(|_| ERR_JSON)?;
        let need = text.len();
        unsafe {
            *out_len = need as u64;
        }
        if out.is_null() || cap < need {
            return Err(3);
        }
        unsafe {
            ptr::copy_nonoverlapping(text.as_ptr(), out, need);
        }
        Ok(())
    })
}

/// Release a handle (idempotent; stale generations read INVALID). Go calls
/// this via runtime.AddCleanup or explicit release — never a double free.
#[no_mangle]
pub extern "C" fn hy_json_release(h: u64) -> i32 {
    with_table(|t| {
        if let Some((idx, gen)) = handle_unpack(h) {
            if let Some(slot) = t.slots.get_mut(idx) {
                if slot.val.is_some() && slot.gen == gen {
                    slot.val = None;
                    slot.gen = next_gen(slot.gen);
                    t.free.push(idx);
                }
            }
        }
        Ok(())
    })
}

/// Live-object count — the bridge's own leak canary (tests assert zero).
#[no_mangle]
pub extern "C" fn hy_json_live_count(out: *mut u64) -> i32 {
    if out.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let live = t.slots.iter().filter(|s| s.val.is_some()).count();
        unsafe {
            *out = live as u64;
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// rust/regex (pinned 1.11.1) — example 09's through-the-package call
// ---------------------------------------------------------------------------

/// `regex.is_match(pattern, text)` — compile + match in one crossing.
/// Returns 1 match / 0 no-match / -1 error (null, invalid UTF-8, or a
/// pattern that does not compile). Status-in-RAX: no out-pointers needed.
#[no_mangle]
pub extern "C" fn hy_regex_is_match(
    pptr: *const u8,
    plen: usize,
    tptr: *const u8,
    tlen: usize,
) -> i64 {
    if plen > isize::MAX as usize
        || tlen > isize::MAX as usize
        || (plen > 0 && pptr.is_null())
        || (tlen > 0 && tptr.is_null())
    {
        return -1;
    }
    let pat = if plen == 0 {
        ""
    } else {
        match std::str::from_utf8(unsafe { std::slice::from_raw_parts(pptr, plen) }) {
            Ok(s) => s,
            Err(_) => return -1,
        }
    };
    let text = if tlen == 0 {
        ""
    } else {
        match std::str::from_utf8(unsafe { std::slice::from_raw_parts(tptr, tlen) }) {
            Ok(s) => s,
            Err(_) => return -1,
        }
    };
    match regex::Regex::new(pat) {
        Ok(re) => {
            if re.is_match(text) {
                1
            } else {
                0
            }
        }
        Err(_) => -1,
    }
}

/// `regex.is_valid(pattern)` — does the pattern compile? 1/0, no error
/// channel (example 09's simple_pattern surface: validity as a bool).
#[no_mangle]
pub extern "C" fn hy_regex_is_valid(pptr: *const u8, plen: usize) -> i64 {
    if plen > isize::MAX as usize || (plen > 0 && pptr.is_null()) {
        return 0;
    }
    let pat = if plen == 0 {
        ""
    } else {
        match std::str::from_utf8(unsafe { std::slice::from_raw_parts(pptr, plen) }) {
            Ok(s) => s,
            Err(_) => return 0,
        }
    };
    if regex::Regex::new(pat).is_ok() {
        1
    } else {
        0
    }
}

/// FNV-1a over the handle table's generation epoch — a cheap handshake word
/// so a Go binary built against a different hyrt aborts loudly, not quietly.
#[no_mangle]
pub extern "C" fn hyrt_abi_word() -> u64 {
    // Bump when an export's signature/semantics changes (E0001-style
    // discipline for the ABI itself). v1: the Batch F/G surface.
    0x68795f61626931 // "hy_abi1"
}

#[cfg(test)]
#[path = "tests/ffi_slice_and_handle_safety.rs"]
mod ffi_slice_and_handle_safety;

#[cfg(test)]
mod json_text_tests {
    use super::{
        hy_json_parse, hy_json_release, hy_json_to_string_pretty, hy_json_value_text, ERR_TABLE,
    };
    use std::ptr;

    type Serializer = extern "C" fn(u64, *mut u8, usize, *mut u64) -> i32;

    fn rejects_invalid_handles(serialize: Serializer) {
        for handle in [0, u64::MAX, 0x0000_0001_ffff_ffff] {
            let mut out = [0xa5; 32];
            let mut need = 0xfeed;
            let status = serialize(handle, out.as_mut_ptr(), out.len(), &mut need);
            eprintln!("handle={handle:#x} status={status} out_len={need:#x}");
            assert_eq!(
                status, ERR_TABLE,
                "invalid handle must return a table error"
            );
            assert_eq!(out, [0xa5; 32]);
            assert_eq!(need, 0xfeed);

            assert_eq!(serialize(handle, ptr::null_mut(), 0, &mut need), ERR_TABLE);
            assert_eq!(need, 0xfeed);
        }
    }

    fn keeps_buffer_protocol(serialize: Serializer, expected: &[u8]) {
        let raw = br#"{"k":7}"#;
        let mut handle = 0;
        assert_eq!(
            hy_json_parse(raw.as_ptr() as u64, raw.len() as u64, &mut handle),
            0
        );
        let mut need = 0;
        assert_eq!(serialize(handle, ptr::null_mut(), 0, &mut need), 3);
        assert_eq!(need as usize, expected.len());

        let mut out = [0xa5; 64];
        assert_eq!(
            serialize(handle, out.as_mut_ptr(), expected.len() - 1, &mut need),
            3
        );
        assert_eq!(need as usize, expected.len());
        assert_eq!(out, [0xa5; 64]);

        assert_eq!(
            serialize(
                handle,
                out.as_mut_ptr().wrapping_add(1),
                expected.len(),
                &mut need
            ),
            0
        );
        assert_eq!(need as usize, expected.len());
        assert_eq!(&out[1..expected.len() + 1], expected);
        assert_eq!(out[0], 0xa5);
        assert!(out[expected.len() + 1..].iter().all(|&b| b == 0xa5));

        let mut guarded = [0xa5; 64];
        assert_eq!(
            serialize(handle, guarded.as_mut_ptr(), guarded.len(), ptr::null_mut()),
            ERR_TABLE
        );
        assert_eq!(guarded, [0xa5; 64]);
        assert_eq!(hy_json_release(handle), 0);
    }

    #[test]
    fn compact_rejects_invalid_handles() {
        rejects_invalid_handles(hy_json_value_text);
    }

    #[test]
    fn pretty_rejects_invalid_handles() {
        rejects_invalid_handles(hy_json_to_string_pretty);
    }

    #[test]
    fn compact_preserves_query_short_buffer_and_copy_protocol() {
        keeps_buffer_protocol(hy_json_value_text, br#"{"k":7}"#);
    }

    #[test]
    fn pretty_preserves_query_short_buffer_and_copy_protocol() {
        keeps_buffer_protocol(hy_json_to_string_pretty, b"{\n  \"k\": 7\n}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_reuse_increments_generation_and_ignores_stale_cleanup_release() {
        let mut doc = 0u64;
        let mut child = 0u64;
        assert_eq!(hy_json_new_object(&mut doc), 0);
        assert_eq!(hy_json_new_array(&mut child), 0);
        let item = b"ada";
        assert_eq!(
            hy_json_array_push_string(child, item.as_ptr() as u64, item.len() as u64),
            0
        );

        let key = b"active";
        // Moving `child` into `doc["active"]` frees `child`'s slot and advances its generation.
        assert_eq!(
            hy_json_set(doc, key.as_ptr() as u64, key.len() as u64, child),
            0
        );

        // Allocate until `child`'s freed slot index is reused (other unit tests run in
        // parallel against the global table and may push/pop freelist entries concurrently).
        let child_idx = handle_unpack(child).map(|(idx, _)| idx);
        let raw = b"{\"k\":99}";
        let mut held = Vec::new();
        let mut next = 0u64;
        for _ in 0..64 {
            let mut candidate = 0u64;
            assert_eq!(
                hy_json_parse(raw.as_ptr() as u64, raw.len() as u64, &mut candidate),
                0
            );
            if handle_unpack(candidate).map(|(idx, _)| idx) == child_idx {
                next = candidate;
                break;
            }
            held.push(candidate);
        }
        for h in held {
            assert_eq!(hy_json_release(h), 0);
        }
        assert_ne!(next, 0, "freelist should reuse child's slot index");
        assert_ne!(
            next, child,
            "reused slot must carry an incremented generation tag (GUARANTEES row 19)"
        );
        assert_eq!(
            handle_unpack(next).map(|(idx, _)| idx),
            child_idx,
            "freelist should reuse the same slot index"
        );

        // Simulate Go runtime.AddCleanup firing late with the pre-move `child` handle ID:
        // stale generation release must be a no-op and must NOT destroy `next`.
        assert_eq!(hy_json_release(child), 0);

        let mut got = 0i64;
        let k = b"k";
        assert_eq!(
            hy_json_get_int(next, k.as_ptr() as u64, k.len() as u64, &mut got),
            0,
            "late release of moved handle destroyed the live reused slot"
        );
        assert_eq!(got, 99);

        assert_eq!(hy_json_release(next), 0);
        assert_eq!(hy_json_release(doc), 0);
    }

    #[test]
    fn failed_set_on_non_object_does_not_consume_value_handle() {
        let mut arr = 0u64;
        let mut child = 0u64;
        let raw = b"{\"x\":7}";
        assert_eq!(hy_json_new_array(&mut arr), 0);
        assert_eq!(
            hy_json_parse(raw.as_ptr() as u64, raw.len() as u64, &mut child),
            0
        );

        let key = b"k";
        // Setting a key on a JSON array fails with ERR_JSON and must leave `child` intact.
        assert_eq!(
            hy_json_set(arr, key.as_ptr() as u64, key.len() as u64, child),
            ERR_JSON
        );

        let mut got = 0i64;
        let x = b"x";
        assert_eq!(
            hy_json_get_int(child, x.as_ptr() as u64, x.len() as u64, &mut got),
            0,
            "failed hy_json_set must not consume or drop the value handle"
        );
        assert_eq!(got, 7);

        assert_eq!(hy_json_release(child), 0);
        assert_eq!(hy_json_release(arr), 0);
    }
}
