#[path = "../src/lib.rs"]
mod hyrt_impl;

#[test]
fn hyrt_rejects_max_gen_and_validates_push_string_slice_first_3457() {
    let mut out = 0u64;
    assert_eq!(hyrt_impl::hy_json_array_len(0xFFFF_FFFF_0000_0001, &mut out), 1);
    assert_eq!(hyrt_impl::hy_json_array_get(0, u64::MAX, &mut out), 1);
    let mut obj = 0u64;
    assert_eq!(hyrt_impl::hy_json_new_object(&mut obj), 0);
    assert_eq!(hyrt_impl::hy_json_array_push_string(obj, 0, 4), 1);
    assert_eq!(hyrt_impl::hy_json_release(obj), 0);
}
