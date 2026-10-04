use serde_json::json;
use std::{
    ffi::{CStr, CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
};

type Emit = unsafe extern "C" fn(*mut c_void, i32, *const c_char);
type Cancel = unsafe extern "C" fn(*mut c_void) -> i32;
struct MediaCallbacks {
    emit: Emit,
    cancel: Cancel,
    state: *mut c_void,
}
impl dvda_native::media::Callbacks for MediaCallbacks {
    fn emit(&mut self, stream: i32, text: &str) {
        let text = CString::new(text.replace('\0', "\\0")).expect("NUL escaped");
        // SAFETY: caller guarantees callback and state remain live for this call.
        unsafe {
            (self.emit)(self.state, stream, text.as_ptr());
        }
    }
    fn cancelled(&mut self) -> bool {
        unsafe { (self.cancel)(self.state) != 0 }
    }
}

/// Execute one typed media job, with synchronous borrowed callbacks.
/// # Safety
/// request is a live NUL-terminated UTF-8 JSON string. Callbacks and state must
/// remain valid throughout this call and must not unwind across the C boundary.
/// Release the returned string exactly once with dvda_rust_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_media_run(
    request: *const c_char,
    emit: Option<Emit>,
    cancel: Option<Cancel>,
    state: *mut c_void,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<serde_json::Value, String> {
        if request.is_null() {
            return Err("Null media job".into());
        }
        let (Some(emit), Some(cancel)) = (emit, cancel) else {
            return Err("Null media callback".into());
        };
        let text = unsafe { CStr::from_ptr(request) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let job = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut callbacks = MediaCallbacks {
            emit,
            cancel,
            state,
        };
        Ok(json!(dvda_core::media::execute(job, &mut callbacks)))
    }));
    let response = match result {
        Ok(Ok(value)) => json!({"ok":true,"value":value}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Rust panic caught at media ABI boundary"}),
    };
    CString::new(response.to_string())
        .expect("JSON contains no raw NUL")
        .into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn dvda_rust_abi_version() -> u32 {
    1
}

/// Execute one in-process image job with synchronous borrowed callbacks.
/// # Safety
/// request must be live NUL-terminated UTF-8 JSON. Callback pointers and state
/// remain valid during this call and callbacks must not unwind. Free the result
/// exactly once using dvda_rust_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_image_run(
    request: *const c_char,
    emit: Option<Emit>,
    cancel: Option<Cancel>,
    state: *mut c_void,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<serde_json::Value, String> {
        if request.is_null() {
            return Err("Null image job".into());
        }
        let (Some(emit), Some(cancel)) = (emit, cancel) else {
            return Err("Null image callback".into());
        };
        let text = unsafe { CStr::from_ptr(request) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let job = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut callbacks = MediaCallbacks {
            emit,
            cancel,
            state,
        };
        Ok(json!(dvda_core::images::execute(job, &mut callbacks)))
    }));
    let response = match result {
        Ok(Ok(value)) => json!({"ok":true,"value":value}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Rust panic caught at image ABI boundary"}),
    };
    CString::new(response.to_string())
        .expect("JSON contains no raw NUL")
        .into_raw()
}

/// Execute one encoder job. The C codec owns encoded bytes; no postprocessing.
/// # Safety
/// request must be live NUL-terminated UTF-8 JSON. Callback pointers and state
/// remain valid during this call and callbacks must not unwind. Free the result
/// exactly once using dvda_rust_free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_encoder_run(
    request: *const c_char,
    emit: Option<Emit>,
    cancel: Option<Cancel>,
    state: *mut c_void,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<serde_json::Value, String> {
        if request.is_null() {
            return Err("Null encoder job".into());
        }
        let (Some(emit), Some(cancel)) = (emit, cancel) else {
            return Err("Null encoder callback".into());
        };
        let text = unsafe { CStr::from_ptr(request) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let job = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut callbacks = MediaCallbacks {
            emit,
            cancel,
            state,
        };
        Ok(json!(dvda_core::encoder::execute(job, &mut callbacks)))
    }));
    let response = match result {
        Ok(Ok(value)) => json!({"ok":true,"value":value}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Rust panic caught at encoder ABI boundary"}),
    };
    CString::new(response.to_string())
        .expect("JSON contains no raw NUL")
        .into_raw()
}

/// Scan fixed-size records without copying audio through the JSON bridge.
/// Returns null on success or an owned UTF-8 error released with dvda_rust_free.
/// # Safety
/// Nonempty input and output must point to distinct readable/writable buffers
/// of the supplied lengths. Output must be aligned for i64 and remain alive
/// throughout this call. Neither buffer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_aob_scan(
    data: *const u8,
    length: usize,
    stride: usize,
    require_pack: u32,
    output: *mut i64,
    capacity: usize,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        if stride == 0
            || !length.is_multiple_of(stride)
            || capacity != length / stride
            || length > isize::MAX as usize
            || capacity > isize::MAX as usize / 8
            || require_pack > 1
            || (length != 0 && data.is_null())
            || (capacity != 0 && (output.is_null() || !(output as usize).is_multiple_of(8)))
        {
            return Err("Invalid AOB binary ABI arguments".into());
        }
        let input = if length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, length) }
        };
        let output = if capacity == 0 {
            &mut []
        } else {
            unsafe { std::slice::from_raw_parts_mut(output, capacity) }
        };
        dvda_core::aob::scan_pts(input, stride, require_pack != 0, output)
    }));
    let error = match result {
        Ok(Ok(())) => return std::ptr::null_mut(),
        Ok(Err(error)) => error,
        Err(_) => "Rust panic caught at AOB ABI boundary".to_owned(),
    };
    CString::new(error.replace('\0', "\\0"))
        .expect("NUL escaped")
        .into_raw()
}

/// Observe one sector while preserving immediate audit termination on missing PTS.
/// Returns null on success; errors must be released with dvda_rust_free.
/// # Safety
/// Input must be readable for length bytes. State and drop_index must be valid,
/// aligned, disjoint writable pointers and must not overlap the input buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_aob_observe(
    data: *const u8,
    length: usize,
    state: *mut dvda_core::aob::AuditState,
    drop_index: *mut i32,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        if length > isize::MAX as usize
            || (length != 0 && data.is_null())
            || state.is_null()
            || !(state as usize).is_multiple_of(8)
            || drop_index.is_null()
            || !(drop_index as usize).is_multiple_of(4)
        {
            return Err("Invalid AOB audit ABI arguments".into());
        }
        let input = if length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, length) }
        };
        let mut next = unsafe { *state };
        if next.sector_count < 0
            || next.missing_sector < -1
            || next.missing_sector >= next.sector_count
        {
            return Err("Invalid AOB audit state".into());
        }
        let drop = next.observe(input)?;
        unsafe {
            *state = next;
            *drop_index = drop;
        }
        Ok(())
    }));
    let error = match result {
        Ok(Ok(())) => return std::ptr::null_mut(),
        Ok(Err(error)) => error,
        Err(_) => "Rust panic caught at AOB audit ABI boundary".into(),
    };
    CString::new(error.replace('\0', "\\0"))
        .expect("NUL escaped")
        .into_raw()
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
/// pointer must be null or a live owned string returned by this DLL, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dvda_rust_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        drop(unsafe { CString::from_raw(pointer) });
    }
}
