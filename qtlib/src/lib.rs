use libc::c_char;
use piratepaperlib::paper::{self, SeedSource, WalletOptions};
use std::ffi::{CStr, CString};

/**
 * Call into rust to generate a paper wallet. Returns the paper wallet in JSON form.
 * The returned string is owned by Rust; call rust_free_string after use.
 * A null pointer means generation failed (including unavailable OS randomness).
 */
/// # Safety
/// `entropy` must be null or a valid NUL-terminated string for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn rust_generate_wallet(count: u32, entropy: *const c_char) -> *mut c_char {
    if entropy.is_null() {
        return std::ptr::null_mut();
    }
    let generated = std::panic::catch_unwind(|| {
        let entropy = unsafe { CStr::from_ptr(entropy) };
        let options = WalletOptions {
            count,
            ..WalletOptions::default()
        };
        paper::generate_wallet(SeedSource::Random(entropy.to_bytes()), options)
            .and_then(|wallets| paper::to_json(&wallets))
    });
    match generated {
        Ok(Ok(json)) => CString::new(json).map_or(std::ptr::null_mut(), CString::into_raw),
        _ => std::ptr::null_mut(),
    }
}

/**
 * Callers that recieve string return values from other functions should call this to return the string
 * back to rust, so it can be freed. Failure to call this function will result in a memory leak
 */
/// # Safety
/// `s` must be null or an unfreed pointer returned by `rust_generate_wallet`.
#[no_mangle]
pub unsafe extern "C" fn rust_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}
