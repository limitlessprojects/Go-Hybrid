use std::alloc::{alloc, dealloc, Layout};
use std::ptr;
use std::slice;
use std::str;

#[no_mangle]
pub extern "C" fn hy_wasm_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return ptr::NonNull::<u8>::dangling().as_ptr();
    }

    let layout = match Layout::array::<u8>(len) {
        Ok(layout) => layout,
        Err(_) => return ptr::null_mut(),
    };
    unsafe { alloc(layout) }
}

#[no_mangle]
pub unsafe extern "C" fn hy_wasm_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    if let Ok(layout) = Layout::array::<u8>(len) {
        dealloc(ptr, layout);
    }
}

unsafe fn pack_check_output(diags: &[hy_diag::Diagnostic], summary: &hy_diag::CheckSummary) -> u64 {
    let json = hy_diag::check_output_json(diags, summary);
    let bytes = json.as_bytes();
    let len = bytes.len();
    let ptr = hy_wasm_alloc(len);

    if ptr.is_null() {
        return 0;
    }

    ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len);
    ((ptr as u32 as u64) << 32) | (len as u64)
}

/// Takes source code, checks it, returns a pointer to a JSON envelope in memory.
/// Returns (ptr, len) packed as u64.
#[no_mangle]
pub unsafe extern "C" fn hy_wasm_check(src_ptr: *const u8, src_len: usize) -> u64 {
    let slice = if src_len == 0 {
        &[]
    } else {
        if src_ptr.is_null() {
            return 0;
        }
        slice::from_raw_parts(src_ptr, src_len)
    };

    let src = match str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => {
            let diag = hy_diag::Diagnostic::error(
                "HYB-E0002",
                "input.hy",
                hy_diag::Span::point(hy_diag::Pos::new(1, 1)),
                "source is not valid UTF-8",
            );
            let diags = vec![diag];
            let summary = hy_diag::CheckSummary {
                file: "input.hy".into(),
                verdict: "reject".into(),
                errors: 1,
                warnings: 0,
                managed: None,
            };
            return pack_check_output(&diags, &summary);
        }
    };

    let (diags, summary) = match hy_syntax::parse_file("input.hy", src) {
        Ok(ast) => {
            let res = hy_check::check_file(&ast);
            let errors = res
                .diagnostics
                .iter()
                .filter(|d| d.severity == hy_diag::Severity::Error)
                .count();
            let warnings = res
                .diagnostics
                .iter()
                .filter(|d| d.severity == hy_diag::Severity::Warning)
                .count();
            let summary = hy_diag::CheckSummary {
                file: "input.hy".into(),
                verdict: res.verdict.as_str().to_string(),
                errors,
                warnings,
                managed: None,
            };
            (res.diagnostics, summary)
        }
        Err(diags) => {
            let summary = hy_diag::CheckSummary {
                file: "input.hy".into(),
                verdict: "reject".into(),
                errors: diags.len(),
                warnings: 0,
                managed: None,
            };
            (diags, summary)
        }
    };

    pack_check_output(&diags, &summary)
}
