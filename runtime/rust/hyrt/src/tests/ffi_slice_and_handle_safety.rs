//! Regression tests for `hyrt` FFI slice preconditions and JSON handle-table safety:
//! 1. `hy_sha3_digest256(ptr::null(), 0, out)` must succeed without calling
//!    `std::slice::from_raw_parts(ptr::null(), 0)` (which violates Rust's
//!    non-null precondition when Go passes `dataPtr == 0, len == 0` for `[]byte{}`).
//! 2. `len > isize::MAX as usize` is rejected before `std::slice::from_raw_parts`
//!    in `hy_sha3_digest256`, `HyStr::as_str`, `hy_regex_is_match`, and
//!    `hy_regex_is_valid`.
//! 3. JSON handle-table operations validate handles (`ERR_TABLE`) before
//!    inspecting key/string UTF-8 (`ERR_JSON`), and `next_gen` wraps before
//!    `u32::MAX` so `handle_pack` never collides with the `u64::MAX` sentinel.

use super::{
    ERR_JSON, ERR_TABLE, handle_pack, handle_unpack, hy_json_array_push_string, hy_json_get_array,
    hy_json_get_int, hy_json_get_string, hy_json_new_object, hy_json_parse, hy_json_release,
    hy_json_set, hy_regex_is_match, hy_regex_is_valid, hy_sha3_digest256, next_gen,
};
use std::ptr;

#[test]
fn sha3_digest256_accepts_null_pointer_when_len_is_zero() {
    // Go's `sha3binding.Digest256([]byte{})` passes `(0, 0, &out[0])`.
    // Previously `hy_sha3_digest256` called `slice::from_raw_parts(null, 0)`,
    // triggering Rust's non-null precondition panic / UB.
    let mut out = [0u8; 32];
    assert_eq!(hy_sha3_digest256(ptr::null(), 0, out.as_mut_ptr()), 0);
    // NIST SHA3-256("") = a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a
    let expected: [u8; 32] = [
        0xa7, 0xff, 0xc6, 0xf8, 0xbf, 0x1e, 0xd7, 0x66, 0x51, 0xc1, 0x47, 0x56, 0xa0, 0x61, 0xd6,
        0x62, 0xf5, 0x80, 0xff, 0x4d, 0xe4, 0x3b, 0x49, 0xfa, 0x82, 0xd8, 0x0a, 0x4b, 0x80, 0xf8,
        0x43, 0x4a,
    ];
    assert_eq!(out, expected);
}

#[test]
fn ffi_exports_reject_slice_lengths_exceeding_isize_max() {
    let byte = 0x61u8;
    let mut out = [0u8; 32];
    let over = (isize::MAX as usize) + 1;

    assert_eq!(hy_sha3_digest256(&byte, over, out.as_mut_ptr()), 1);
    assert_eq!(hy_regex_is_match(&byte, over, &byte, 1), -1);
    assert_eq!(hy_regex_is_match(&byte, 1, &byte, over), -1);
    assert_eq!(hy_regex_is_valid(&byte, over), 0);

    let mut handle = 0u64;
    assert_eq!(
        hy_json_parse(&byte as *const u8 as u64, over as u64, &mut handle),
        ERR_TABLE
    );
}

#[test]
fn json_ops_validate_handle_before_key_utf8_and_preserve_value_on_bad_key() {
    let bad_utf8 = [0xffu8, 0xfe];
    let kptr = bad_utf8.as_ptr() as u64;
    let klen = bad_utf8.len() as u64;

    for invalid_handle in [0u64, u64::MAX, 0x0000_0001_00ff_ffff] {
        let mut out_h = 0u64;
        let mut out_i = 0i64;
        assert_eq!(
            hy_json_get_array(invalid_handle, kptr, klen, &mut out_h),
            ERR_TABLE
        );
        assert_eq!(
            hy_json_get_int(invalid_handle, kptr, klen, &mut out_i),
            ERR_TABLE
        );
        assert_eq!(
            hy_json_get_string(invalid_handle, kptr, klen, &mut out_h),
            ERR_TABLE
        );
        assert_eq!(
            hy_json_array_push_string(invalid_handle, kptr, klen),
            ERR_TABLE
        );
    }

    // When `v` and `value` are valid handles in `hy_json_set` but the key is
    // invalid UTF-8 (`ERR_JSON`), `value` must not be consumed.
    let mut obj = 0u64;
    let mut child = 0u64;
    let raw = br#"{"n":42}"#;
    assert_eq!(hy_json_new_object(&mut obj), 0);
    assert_eq!(
        hy_json_parse(raw.as_ptr() as u64, raw.len() as u64, &mut child),
        0
    );
    assert_eq!(hy_json_set(obj, kptr, klen, child), ERR_JSON);

    let mut got = 0i64;
    let k = b"n";
    assert_eq!(
        hy_json_get_int(child, k.as_ptr() as u64, k.len() as u64, &mut got),
        0
    );
    assert_eq!(got, 42);
    assert_eq!(hy_json_release(child), 0);
    assert_eq!(hy_json_release(obj), 0);
}

#[test]
fn generation_wrap_never_produces_u64_max_sentinel() {
    let wrapped = next_gen(u32::MAX - 1);
    assert_eq!(wrapped, 1);
    let max_gen = u32::MAX - 1;
    let h = handle_pack((u32::MAX - 2) as usize, max_gen);
    assert_ne!(h, u64::MAX);
    assert_eq!(handle_unpack(h), Some(((u32::MAX - 2) as usize, max_gen)));
}
