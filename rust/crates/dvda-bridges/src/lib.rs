#![allow(unsafe_op_in_unsafe_fn)]
#[cfg(all(feature = "author-loader", not(feature = "image")))]
pub mod author_loader;
#[cfg(any(feature = "media", feature = "image"))]
pub mod direct;
#[cfg(feature = "media")]
pub mod ffmpeg;
#[cfg(feature = "image")]
pub mod image;
#[cfg(feature = "author-iso")]
pub mod iso;
#[cfg(feature = "media")]
pub mod media;
#[cfg(feature = "media")]
mod media_convert;
#[cfg(feature = "menu")]
pub mod menu;
#[cfg(feature = "media")]
pub mod menu_media;
pub mod win;
use std::ffi::{c_char, c_void};
pub type Emit = Option<unsafe extern "C" fn(*mut c_void, i32, *const c_char)>;
pub type Cancel = Option<unsafe extern "C" fn(*mut c_void) -> i32>;
#[derive(Clone, Copy)]
pub struct Call {
    pub emit: Emit,
    pub cancel: Cancel,
    pub state: *mut c_void,
}
impl Call {
    pub fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|f| unsafe { f(self.state) != 0 })
    }
    pub fn message(&self, stream: i32, text: &str) {
        let s = std::ffi::CString::new(text.replace('\0', "")).unwrap();
        if let Some(f) = self.emit {
            unsafe { f(self.state, stream, s.as_ptr()) }
        } else if stream == 2 {
            eprint!("{text}")
        } else {
            print!("{text}")
        }
    }
}
