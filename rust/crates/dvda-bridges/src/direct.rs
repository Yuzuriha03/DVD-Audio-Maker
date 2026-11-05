//! Owned, directly linked entry points. No project-owned DLL is loaded here.
use std::ffi::CString;
#[cfg(feature = "media")]
use std::ffi::NulError;

#[cfg(feature = "media")]
#[derive(Clone, Debug)]
pub struct MediaRequest {
    pub operation: u32,
    pub rate: u32,
    pub bits: u32,
    pub output_format: u32,
    pub soxr: bool,
    pub dither: bool,
    pub compression: u32,
    pub cover: bool,
    pub input: CString,
    pub output: Option<CString>,
    pub tags: Vec<CString>,
}

#[cfg(feature = "media")]
impl MediaRequest {
    pub fn new(operation: u32, input: &str, output: &str) -> Result<Self, NulError> {
        Ok(Self {
            operation,
            rate: 0,
            bits: 0,
            output_format: 0,
            soxr: false,
            dither: true,
            compression: 0,
            cover: false,
            input: CString::new(input)?,
            output: Some(CString::new(output)?),
            tags: Vec::new(),
        })
    }

    pub fn run(&self) -> i32 {
        unsafe {
            self.run_with_callbacks(crate::Call {
                emit: None,
                cancel: None,
                state: std::ptr::null_mut(),
            })
        }
    }

    /// # Safety
    /// Callback state must remain valid for the call. Callbacks must not unwind.
    pub unsafe fn run_with_callbacks(&self, callbacks: crate::Call) -> i32 {
        extern "C" fn discard(_: *mut std::ffi::c_void, _: i32, _: *const std::ffi::c_char) {}
        self.with_raw_request(|request| unsafe {
            crate::media::dvdamedia_run(
                request,
                callbacks.emit.or(Some(discard)),
                callbacks.cancel,
                callbacks.state,
            )
        })
    }

    /// # Safety
    /// Callback and sink state must remain valid for the synchronous call and must not unwind.
    pub unsafe fn run_pcm_with_callbacks(
        &self,
        mut callbacks: crate::Call,
        sink: &mut dyn crate::media_convert::PcmSink,
    ) -> i32 {
        self.with_raw_request(|request| unsafe {
            crate::media::audio_sink(request, &mut callbacks, sink)
        })
    }

    fn with_raw_request(&self, run: impl FnOnce(&crate::media::Request) -> i32) -> i32 {
        if !self.tags.len().is_multiple_of(2) || self.tags.len() / 2 > u32::MAX as usize {
            return -22;
        }
        let tags: Vec<_> = self.tags.iter().map(|tag| tag.as_ptr()).collect();
        let request = crate::media::Request {
            size: std::mem::size_of::<crate::media::Request>() as u32,
            abi: 2,
            operation: self.operation,
            rate: self.rate,
            bits: self.bits,
            output_format: self.output_format,
            soxr: self.soxr as u32,
            dither: self.dither as u32,
            compression: self.compression,
            cover: self.cover as u32,
            tag_count: (tags.len() / 2) as u32,
            input: self.input.as_ptr(),
            output: self
                .output
                .as_ref()
                .map_or(std::ptr::null(), |path| path.as_ptr()),
            tags: if tags.is_empty() {
                std::ptr::null()
            } else {
                tags.as_ptr()
            },
        };
        run(&request)
    }
}

#[cfg(all(test, feature = "media"))]
mod media_request_tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn raw_request_preserves_missing_and_empty_output() {
        let mut owned = MediaRequest::new(1, "input.wav", "").unwrap();
        owned.output = None;
        assert_eq!(
            owned.with_raw_request(|raw| {
                assert!(raw.output.is_null());
                assert!(raw.tags.is_null());
                assert_eq!(raw.tag_count, 0);
                17
            }),
            17
        );
        owned.output = Some(CString::new("").unwrap());
        owned.tags = vec![
            CString::new("title").unwrap(),
            CString::new("song").unwrap(),
        ];
        owned.with_raw_request(|raw| {
            assert!(!raw.output.is_null());
            assert!(unsafe { CStr::from_ptr(raw.output) }.to_bytes().is_empty());
            assert_eq!(raw.tag_count, 1);
            assert_eq!(unsafe { CStr::from_ptr(*raw.tags) }.to_bytes(), b"title");
            assert_eq!(
                unsafe { CStr::from_ptr(*raw.tags.add(1)) }.to_bytes(),
                b"song"
            );
            0
        });
    }

    #[test]
    fn odd_tags_are_rejected_before_dispatch() {
        let mut owned = MediaRequest::new(1, "input.wav", "").unwrap();
        owned.tags.push(CString::new("title").unwrap());
        assert_eq!(
            owned.with_raw_request(|_| panic!("invalid request dispatched")),
            -22
        );
    }
}
