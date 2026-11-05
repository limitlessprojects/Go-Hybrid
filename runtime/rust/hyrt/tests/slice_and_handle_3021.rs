#[path = "../src/lib.rs"]
mod hyrt_impl;

#[test]
fn hyrt_null_empty_slice_and_handle_order_3021() {
    let mut digest = [0u8; 32];
    assert_eq!(hyrt_impl::hy_sha3_digest256(std::ptr::null(), 0, digest.as_mut_ptr()), 0);
    assert_eq!(hyrt_impl::hy_sha3_digest256(digest.as_ptr(), (isize::MAX as usize) + 1, digest.as_mut_ptr()), 1);
    assert_eq!(hyrt_impl::hy_regex_is_valid(std::ptr::null(), (isize::MAX as usize) + 1), 0);
    let mut out_int = 0i64;
    assert_eq!(hyrt_impl::hy_json_get_int(0, 0, (isize::MAX as u64) + 1, &mut out_int), 1);
}