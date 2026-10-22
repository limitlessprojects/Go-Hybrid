//! hyrt — the Hybrid runtime bridge (Phase 2, Batch F).
//!
//! One staticlib / Wasm module per binary. Every export is
//! `#[no_mangle] pub extern "C"`, takes u32/u64/pointers,
//! never unwinds across boundaries, and executes safely on the Go stack when
//! transpiled.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ptr;

// ---------------------------------------------------------------------------
// Wasm Memory Allocation Exports
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hyrt_alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn hyrt_free(ptr: *mut u8, size: usize) {
    if !ptr.is_null() && size > 0 {
        unsafe {
            let _ = Vec::from_raw_parts(ptr, 0, size);
        }
    }
}

// ---------------------------------------------------------------------------
// rust/sha3 (pinned 0.10.8)
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hy_sha3_digest256(
    data: *const u8,
    len: usize,
    out: *mut u8, /* 32 */
) -> i32 {
    if out.is_null() || (len > 0 && data.is_null()) {
        return 1;
    }
    use sha3::{Digest, Sha3_256};
    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    let digest = Sha3_256::digest(slice);
    unsafe {
        ptr::copy_nonoverlapping(digest.as_ptr(), out, 32);
    }
    0
}

#[no_mangle]
pub extern "C" fn hy_sha3_stream_checksum(iters: u64, seed: u64) -> u64 {
    use sha3::{Digest, Sha3_256};
    let mut state: u64 = seed ^ 0x68796272_69645f72;
    let mut buf = [0u8; 8];
    for i in 0..iters {
        buf.copy_from_slice(&state.to_le_bytes());
        let d = Sha3_256::digest(buf);
        let word = u64::from_le_bytes(d[0..8].try_into().unwrap());
        state = state.rotate_left(13) ^ word ^ i;
    }
    state
}

// ---------------------------------------------------------------------------
// rust/sha2 (pinned 0.10.8)
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hy_sha2_256(
    data: *const u8,
    len: usize,
    out: *mut u8, /* 32 */
) -> i32 {
    if out.is_null() || (len > 0 && data.is_null()) {
        return 1;
    }
    use sha2::{Digest, Sha256};
    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    let digest = Sha256::digest(slice);
    unsafe {
        ptr::copy_nonoverlapping(digest.as_ptr(), out, 32);
    }
    0
}

// ---------------------------------------------------------------------------
// rust/flate2 (pinned 1.0.35)
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hy_flate_compress(
    data: *const u8,
    len: usize,
    out: *mut u8,
    cap: usize,
    out_len: *mut usize,
) -> i32 {
    if data.is_null() || out_len.is_null() {
        return 1;
    }
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    if encoder.write_all(slice).is_err() {
        return 1;
    }
    let res = match encoder.finish() {
        Ok(b) => b,
        Err(_) => return 1,
    };
    unsafe {
        *out_len = res.len();
    }
    if res.len() > cap || out.is_null() {
        return 3;
    }
    unsafe {
        ptr::copy_nonoverlapping(res.as_ptr(), out, res.len());
    }
    0
}

#[no_mangle]
pub extern "C" fn hy_flate_decompress(
    data: *const u8,
    len: usize,
    out: *mut u8,
    cap: usize,
    out_len: *mut usize,
) -> i32 {
    if data.is_null() || out_len.is_null() {
        return 1;
    }
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    let mut decoder = ZlibDecoder::new(slice);
    let mut decompressed = Vec::new();
    if decoder.read_to_end(&mut decompressed).is_err() {
        return 1;
    }
    unsafe {
        *out_len = decompressed.len();
    }
    if decompressed.len() > cap || out.is_null() {
        return 3;
    }
    unsafe {
        ptr::copy_nonoverlapping(decompressed.as_ptr(), out, decompressed.len());
    }
    0
}

// ---------------------------------------------------------------------------
// rust/ed25519-dalek (pinned 2.1.1)
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hy_ed25519_sign(
    secret: *const u8, /* 32 bytes */
    msg: *const u8,
    msg_len: usize,
    out_sig: *mut u8, /* 64 bytes */
) -> i32 {
    if secret.is_null() || out_sig.is_null() || (msg_len > 0 && msg.is_null()) {
        return 1;
    }
    use ed25519_dalek::{Signer, SigningKey};
    let secret_bytes = unsafe { std::slice::from_raw_parts(secret, 32) };
    let key_arr: [u8; 32] = match secret_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => return 1,
    };
    let signing_key = SigningKey::from_bytes(&key_arr);
    let msg_slice = unsafe { std::slice::from_raw_parts(msg, msg_len) };
    let signature = signing_key.sign(msg_slice);
    unsafe {
        ptr::copy_nonoverlapping(signature.to_bytes().as_ptr(), out_sig, 64);
    }
    0
}

#[no_mangle]
pub extern "C" fn hy_ed25519_verify(
    pubkey: *const u8, /* 32 bytes */
    msg: *const u8,
    msg_len: usize,
    sig: *const u8, /* 64 bytes */
) -> i32 {
    if pubkey.is_null() || sig.is_null() || (msg_len > 0 && msg.is_null()) {
        return 0;
    }
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let pk_bytes = unsafe { std::slice::from_raw_parts(pubkey, 32) };
    let pk_arr: [u8; 32] = match pk_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => return 0,
    };
    let verifying_key = match VerifyingKey::from_bytes(&pk_arr) {
        Ok(vk) => vk,
        Err(_) => return 0,
    };
    let sig_bytes = unsafe { std::slice::from_raw_parts(sig, 64) };
    let sig_arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(arr) => arr,
        Err(_) => return 0,
    };
    let signature = Signature::from_bytes(&sig_arr);
    let msg_slice = unsafe { std::slice::from_raw_parts(msg, msg_len) };
    if verifying_key.verify(msg_slice, &signature).is_ok() {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hy_nop(x: u64) -> u64 {
    x
}

#[no_mangle]
pub extern "C" fn hyrt_stack_alignment_probe() -> u64 {
    0
}

// ---------------------------------------------------------------------------
// rust/serde_json (pinned 1.0.140)
// ---------------------------------------------------------------------------

use serde_json::{Map, Value as Json};
use std::sync::Mutex;

struct Table {
    slots: Vec<Option<(u32, Json)>>,
    free: Vec<usize>,
}

static TABLE: Mutex<Table> = Mutex::new(Table {
    slots: Vec::new(),
    free: Vec::new(),
});

const ERR_TABLE: i32 = 1;
const ERR_JSON: i32 = 2;

fn handle_pack(idx: usize, gen: u32) -> u64 {
    ((gen as u64) << 32) | (idx as u64 + 1)
}

fn handle_unpack(h: u64) -> Option<(usize, u32)> {
    if h == 0 || h == u64::MAX {
        return None;
    }
    let idx = (h & 0xFFFF_FFFF) as usize - 1;
    let gen = (h >> 32) as u32;
    Some((idx, gen))
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
    let gen = 1u32;
    if let Some(idx) = t.free.pop() {
        t.slots[idx] = Some((gen, v));
        return handle_pack(idx, gen);
    }
    t.slots.push(Some((gen, v)));
    handle_pack(t.slots.len() - 1, gen)
}

fn table_get(t: &Table, h: u64) -> Result<&Json, i32> {
    let (idx, gen) = handle_unpack(h).ok_or(ERR_TABLE)?;
    let slot = t.slots.get(idx).ok_or(ERR_TABLE)?;
    let (g, v) = slot.as_ref().ok_or(ERR_TABLE)?;
    if *g != gen {
        return Err(ERR_TABLE);
    }
    Ok(v)
}

fn table_get_mut(t: &mut Table, h: u64) -> Result<&mut Json, i32> {
    let (idx, gen) = handle_unpack(h).ok_or(ERR_TABLE)?;
    let slot = t.slots.get_mut(idx).ok_or(ERR_TABLE)?;
    let (g, v) = slot.as_mut().ok_or(ERR_TABLE)?;
    if *g != gen {
        return Err(ERR_TABLE);
    }
    Ok(v)
}

fn str_from_raw(ptr: *const u8, len: usize) -> Result<&'static str, i32> {
    if len == 0 {
        return Ok("");
    }
    if ptr.is_null() {
        return Err(ERR_TABLE);
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).map_err(|_| ERR_JSON)
}

#[no_mangle]
pub extern "C" fn hy_json_parse(ptr: *const u8, len: usize, out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    let s = match str_from_raw(ptr, len) {
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

#[no_mangle]
pub extern "C" fn hy_json_get_array(
    v: u64,
    kptr: *const u8,
    klen: usize,
    out_handle: *mut u64,
) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    let k = match str_from_raw(kptr, klen) {
        Ok(k) => k,
        Err(e) => return e,
    };
    with_table(|t| {
        let arr = table_get(t, v)?
            .get(k)
            .ok_or(ERR_JSON)?
            .as_array()
            .ok_or(ERR_JSON)?;
        let h = table_insert(t, Json::Array(arr.clone()));
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn hy_json_get_int(v: u64, kptr: *const u8, klen: usize, out: *mut i64) -> i32 {
    if out.is_null() {
        return ERR_TABLE;
    }
    let k = match str_from_raw(kptr, klen) {
        Ok(k) => k,
        Err(e) => return e,
    };
    with_table(|t| {
        let n = table_get(t, v)?
            .get(k)
            .ok_or(ERR_JSON)?
            .as_i64()
            .ok_or(ERR_JSON)?;
        unsafe {
            *out = n;
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn hy_json_get_string(
    v: u64,
    kptr: *const u8,
    klen: usize,
    out_handle: *mut u64,
) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    let k = match str_from_raw(kptr, klen) {
        Ok(k) => k,
        Err(e) => return e,
    };
    with_table(|t| {
        let s = table_get(t, v)?
            .get(k)
            .ok_or(ERR_JSON)?
            .as_str()
            .ok_or(ERR_JSON)?;
        let h = table_insert(t, Json::String(s.to_string()));
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

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

#[no_mangle]
pub extern "C" fn hy_json_array_get(arr: u64, i: u64, out_handle: *mut u64) -> i32 {
    if out_handle.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let elem = table_get(t, arr)?
            .as_array()
            .ok_or(ERR_JSON)?
            .get(i as usize)
            .ok_or(ERR_JSON)?
            .clone();
        let h = table_insert(t, elem);
        unsafe {
            *out_handle = h;
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn hy_json_array_push_string(arr: u64, sptr: *const u8, slen: usize) -> i32 {
    let sv = match str_from_raw(sptr, slen) {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    with_table(|t| {
        let a = table_get_mut(t, arr)?.as_array_mut().ok_or(ERR_JSON)?;
        a.push(Json::String(sv));
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn hy_json_set(v: u64, kptr: *const u8, klen: usize, value: u64) -> i32 {
    let k = match str_from_raw(kptr, klen) {
        Ok(k) => k.to_string(),
        Err(e) => return e,
    };
    with_table(|t| {
        let (vidx, vgen) = handle_unpack(value).ok_or(ERR_TABLE)?;
        {
            let slot = t.slots.get_mut(vidx).ok_or(ERR_TABLE)?;
            let (g, _) = slot.as_ref().ok_or(ERR_TABLE)?;
            if *g != vgen {
                return Err(ERR_TABLE);
            }
        }
        let (_, val) = t.slots[vidx].take().ok_or(ERR_TABLE)?;
        t.free.push(vidx);
        let obj = table_get_mut(t, v)?.as_object_mut().ok_or(ERR_JSON)?;
        obj.insert(k, val);
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn hy_json_value_text(v: u64, out: *mut u8, cap: usize, out_len: *mut u64) -> i32 {
    if out_len.is_null() {
        return ERR_TABLE;
    }
    let mut status = 0i32;
    with_table(|t| {
        let text = table_get(t, v)?.to_string();
        let need = text.len();
        unsafe {
            *out_len = need as u64;
        }
        if out.is_null() || cap < need {
            status = 3;
            return Ok(());
        }
        unsafe {
            ptr::copy_nonoverlapping(text.as_ptr(), out, need);
        }
        Ok(())
    });
    status
}

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
    let mut status = 0i32;
    with_table(|t| {
        let text = serde_json::to_string_pretty(table_get(t, v)?).map_err(|_| ERR_JSON)?;
        let need = text.len();
        unsafe {
            *out_len = need as u64;
        }
        if out.is_null() || cap < need {
            status = 3;
            return Ok(());
        }
        unsafe {
            ptr::copy_nonoverlapping(text.as_ptr(), out, need);
        }
        Ok(())
    });
    status
}

#[no_mangle]
pub extern "C" fn hy_json_release(h: u64) -> i32 {
    with_table(|t| {
        if let Some((idx, gen)) = handle_unpack(h) {
            if let Some(slot) = t.slots.get_mut(idx) {
                if let Some((g, _)) = slot.as_ref() {
                    if *g == gen {
                        *slot = None;
                        t.free.push(idx);
                    }
                }
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn hy_json_live_count(out: *mut u64) -> i32 {
    if out.is_null() {
        return ERR_TABLE;
    }
    with_table(|t| {
        let live = t.slots.iter().filter(|s| s.is_some()).count();
        unsafe {
            *out = live as u64;
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// rust/regex (pinned 1.11.1)
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn hy_regex_is_match(
    pptr: *const u8,
    plen: usize,
    tptr: *const u8,
    tlen: usize,
) -> i64 {
    if (plen > 0 && pptr.is_null()) || (tlen > 0 && tptr.is_null()) {
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

#[no_mangle]
pub extern "C" fn hy_regex_is_valid(pptr: *const u8, plen: usize) -> i64 {
    if plen > 0 && pptr.is_null() {
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

#[no_mangle]
pub extern "C" fn hyrt_abi_word() -> u64 {
    0x68795f61626931
}
