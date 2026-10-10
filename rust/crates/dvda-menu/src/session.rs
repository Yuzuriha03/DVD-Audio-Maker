//! Transaction lifecycle. Its C core callback owns the complete failure-jump island.
use crate::resources::{self, ReadRgba, Session};
use std::{
    ffi::{c_char, c_int},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
#[repr(C)]
pub struct MenuRequest {
    pub size: u32,
    pub xml: *const c_char,
    pub input: *const c_char,
    pub output: *const c_char,
    pub read_rgba: Option<ReadRgba>,
}
type Core = unsafe extern "C" fn(*const c_char, *const c_char, c_int, c_int) -> c_int;
type Reset = unsafe extern "C" fn();
static CONSUMED: AtomicBool = AtomicBool::new(false);

struct VendorState {
    reset: Option<Reset>,
    error: *mut bool,
    accepts_body: *mut bool,
    body: *mut *mut c_char,
}

impl Drop for VendorState {
    fn drop(&mut self) {
        // Created before Session: every return/unwind drops the heap first.
        unsafe {
            *self.error = false;
            *self.accepts_body = false;
            *self.body = ptr::null_mut();
            if let Some(reset) = self.reset {
                reset();
            }
        }
    }
}
/// # Safety
/// Request, parser globals and callbacks remain valid for this call. `core` catches
/// every vendor longjmp before returning. No callback may unwind or jump across
/// Rust frames. The caller serializes access and discards failed partial output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_run(
    request: *const MenuRequest,
    navigation: c_int,
    core: Option<Core>,
    reset: Option<Reset>,
    parser_error: *mut bool,
    parser_accepts_body: *mut bool,
    parser_body: *mut *mut c_char,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if reset.is_none() && CONSUMED.swap(true, Ordering::AcqRel) {
            return 1;
        }
        let (Some(request), Some(core)) = (unsafe { request.as_ref() }, core) else {
            return 1;
        };
        if request.size as usize != size_of::<MenuRequest>()
            || request.xml.is_null()
            || request.output.is_null()
            || parser_error.is_null()
            || parser_accepts_body.is_null()
            || parser_body.is_null()
        {
            return 1;
        }
        let _vendor_state = VendorState {
            reset,
            error: parser_error,
            accepts_body: parser_accepts_body,
            body: parser_body,
        };
        if let Some(reset) = reset {
            unsafe { reset() }
        }
        let Ok(mut session) = Session::begin(request.read_rgba) else {
            return 1;
        };
        let mut input = -1;
        let mut output = -1;
        let valid = navigation != 0
            || unsafe {
                resources::menu_rust_open(request.input, 0, 0, &mut input) == 0
                    && input >= 0
                    && resources::file_length(input)
                        .is_ok_and(|length| length != 0 && length % 2048 == 0)
                    && resources::menu_rust_open(
                        request.output,
                        0x1 | 0x100 | 0x200, // _O_WRONLY | _O_CREAT | _O_TRUNC
                        0x100 | 0x80,        // _S_IREAD | _S_IWRITE
                        &mut output,
                    ) == 0
                    && output >= 0
            };
        let mut result = if valid {
            unsafe { core(request.xml, request.output, input, output) }
        } else {
            1
        };
        if !session.finish() {
            result = 1
        }
        result
    }))
    .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, ffi::CString};

    thread_local! { static RESETS: Cell<usize> = const { Cell::new(0) }; }
    unsafe extern "C" fn reset() {
        RESETS.set(RESETS.get() + 1);
    }
    unsafe extern "C" fn failing_core(
        _: *const c_char,
        _: *const c_char,
        _: c_int,
        _: c_int,
    ) -> c_int {
        let mut object = ptr::null_mut();
        assert_eq!(unsafe { resources::menu_rust_malloc(64, &mut object) }, 0);
        17
    }

    #[test]
    fn core_failure_and_begin_failure_reset_parser_and_release_heap() {
        let xml = CString::new("fixture.xml").unwrap();
        let output = CString::new("output").unwrap();
        let request = MenuRequest {
            size: size_of::<MenuRequest>() as u32,
            xml: xml.as_ptr(),
            input: ptr::null(),
            output: output.as_ptr(),
            read_rgba: None,
        };
        let mut error = true;
        let mut accepts = true;
        let mut body = ptr::dangling_mut::<c_char>();
        assert_eq!(
            unsafe {
                menu_rust_run(
                    &request,
                    1,
                    Some(failing_core),
                    Some(reset),
                    &mut error,
                    &mut accepts,
                    &mut body,
                )
            },
            17
        );
        assert!(!error && !accepts && body.is_null());
        assert_eq!(RESETS.get(), 2);
        let _busy_session = Session::begin(None).unwrap();
        error = true;
        accepts = true;
        body = ptr::dangling_mut::<c_char>();
        assert_eq!(
            unsafe {
                menu_rust_run(
                    &request,
                    1,
                    Some(failing_core),
                    Some(reset),
                    &mut error,
                    &mut accepts,
                    &mut body,
                )
            },
            1
        );
        assert!(!error && !accepts && body.is_null());
        assert_eq!(RESETS.get(), 4);
    }

    #[test]
    fn rust_panic_drops_heap_before_parser_and_vendor_reset() {
        unsafe extern "C" fn verify_reset() {
            let mut object = ptr::null_mut();
            assert_eq!(unsafe { resources::menu_rust_malloc(1, &mut object) }, 1);
            RESETS.set(RESETS.get() + 1);
        }
        let mut error = true;
        let mut accepts = true;
        let mut body = ptr::dangling_mut::<c_char>();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _vendor = VendorState {
                reset: Some(verify_reset),
                error: &mut error,
                accepts_body: &mut accepts,
                body: &mut body,
            };
            let _session = Session::begin(None).unwrap();
            panic!("injected Rust transaction failure");
        }));
        assert!(result.is_err());
        assert!(!error && !accepts && body.is_null());
        assert_eq!(RESETS.get(), 1);
        assert!(Session::begin(None).is_ok());
    }
}
