//! Menu request ABI shared by the in-process callers.
use crate::resources::ReadRgba;
use std::ffi::c_char;

#[repr(C)]
pub struct MenuRequest {
    pub size: u32,
    pub xml: *const c_char,
    pub input: *const c_char,
    pub output: *const c_char,
    pub read_rgba: Option<ReadRgba>,
}
