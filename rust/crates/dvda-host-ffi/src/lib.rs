use serde_json::json;
use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
};

#[unsafe(no_mangle)]
pub extern "C" fn dvda_rust_abi_version() -> u32 {
    1
}

/// # Safety
/// Both inputs must point to readable NUL-terminated UTF-8 strings for this call.
/// The returned string must be released exactly once with dvda_rust_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_call(
    operation: *const c_char,
    request: *const c_char,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<serde_json::Value, String> {
        if operation.is_null() || request.is_null() {
            return Err("Null ABI argument".into());
        }
        let operation = unsafe { CStr::from_ptr(operation) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let request = unsafe { CStr::from_ptr(request) }
            .to_str()
            .map_err(|e| e.to_string())?;
        dvda_core::dispatch(
            operation,
            serde_json::from_str(request).map_err(|e| e.to_string())?,
        )
    }));
    let envelope = match result {
        Ok(Ok(value)) => json!({"ok":true,"value":value}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Rust panic caught at ABI boundary"}),
    };
    CString::new(envelope.to_string())
        .expect("JSON contains no raw NUL")
        .into_raw()
}

/// # Safety
/// pointer must be null or a live return value of dvda_rust_call, freed only once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        drop(unsafe { CString::from_raw(pointer) });
    }
}
