//! In-process Rust author entry shared by the application and standalone CLI.
use crate::{media::Failure, process::Outcome};
use dvda_native::media::Callbacks;
use std::{path::Path, time::Instant};

pub fn execute(arguments: &[String], caller: &mut dyn Callbacks) -> Outcome {
    let start = Instant::now();
    struct Capture<'a> {
        caller: &'a mut dyn Callbacks,
        output: [String; 2],
    }
    impl dvda_author::Callbacks for Capture<'_> {
        fn emit(&mut self, stream: i32, text: &str) {
            let index = if stream == 2 { 1 } else { 0 };
            self.output[index].push_str(text);
            if !text.ends_with('\n') {
                self.output[index].push('\n');
            }
            for line in text.lines() {
                self.caller.emit(stream, line);
            }
        }
        fn cancelled(&mut self) -> bool {
            self.caller.cancelled()
        }
        fn progress(&mut self, completed: u64, total: u64) {
            self.caller.progress(completed, total);
        }
    }
    let mut capture = Capture {
        caller,
        output: [String::new(), String::new()],
    };
    let result = dvda_author::command::run(arguments, &mut Backend, &mut capture);
    let failure = result.err().map(|message| {
        dvda_author::Callbacks::emit(&mut capture, 2, &message);
        let kind = if dvda_author::Callbacks::cancelled(&mut capture) {
            "OperationCanceled"
        } else {
            "Author"
        };
        Failure::new(kind, &message)
    });
    let [standard_output, standard_error] = capture.output;
    Outcome {
        exit_code: Some(if failure.is_some() { 1 } else { 0 }),
        failure,
        standard_output,
        standard_error,
        duration_seconds: start.elapsed().as_secs_f64(),
    }
}

pub struct Backend;

#[cfg(feature = "direct-bridges")]
fn string(value: &str) -> Result<std::ffi::CString, String> {
    std::ffi::CString::new(value).map_err(|_| "Menu argument contains NUL".into())
}
#[cfg(feature = "direct-bridges")]
fn path(value: &Path) -> Result<std::ffi::CString, String> {
    string(value.to_str().ok_or("Menu path is not valid Unicode")?)
}
#[cfg(feature = "direct-bridges")]
fn status(code: i32, callbacks: &mut dyn dvda_author::Callbacks) -> Result<(), String> {
    if callbacks.cancelled() {
        Err("Author cancelled".into())
    } else if code == 0 {
        Ok(())
    } else {
        Err(format!("Menu backend failed with status {code}"))
    }
}

impl dvda_author::menu::Backend for Backend {
    fn image_run(
        &mut self,
        arguments: Vec<String>,
        callbacks: &mut dyn dvda_author::Callbacks,
    ) -> Result<(), String> {
        #[cfg(feature = "direct-bridges")]
        {
            use std::ffi::{CStr, c_char, c_void};
            struct State<'a>(&'a mut dyn dvda_author::Callbacks);
            unsafe extern "C" fn emit(state: *mut c_void, stream: i32, text: *const c_char) {
                if !text.is_null() {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                        (*(state as *mut State<'_>))
                            .0
                            .emit(stream, &CStr::from_ptr(text).to_string_lossy());
                    }));
                }
            }
            unsafe extern "C" fn cancel(state: *mut c_void) -> i32 {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                    (*(state as *mut State<'_>)).0.cancelled() as i32
                }))
                .unwrap_or(1)
            }
            let arguments = arguments
                .iter()
                .map(|s| string(s))
                .collect::<Result<Vec<_>, _>>()?;
            let mut state = State(callbacks);
            let code = unsafe {
                dvda_bridges::direct::image_run_with_callbacks(
                    &arguments,
                    dvda_bridges::Call {
                        emit: Some(emit),
                        cancel: Some(cancel),
                        state: &mut state as *mut State<'_> as *mut c_void,
                    },
                )
            };
            status(code, callbacks)
        }
        #[cfg(not(feature = "direct-bridges"))]
        {
            let _ = (arguments, callbacks);
            Err("Menu authoring requires the direct-bridges feature".into())
        }
    }
    fn write_y4m(
        &mut self,
        input: &Path,
        output: &Path,
        norm: &str,
        aspect: &str,
        callbacks: &mut dyn dvda_author::Callbacks,
    ) -> Result<(), String> {
        #[cfg(feature = "direct-bridges")]
        {
            status(
                dvda_bridges::direct::write_y4m(
                    &path(input)?,
                    &path(output)?,
                    &string(if norm == "ntsc" { "30" } else { "25" })?,
                    &string(aspect)?,
                ),
                callbacks,
            )
        }
        #[cfg(not(feature = "direct-bridges"))]
        {
            let _ = (input, output, norm, aspect, callbacks);
            Err("Menu authoring requires the direct-bridges feature".into())
        }
    }
    fn create_mpg(
        &mut self,
        y4m: &Path,
        wav: &Path,
        output: &Path,
        norm: &str,
        aspect: &str,
        still: bool,
        callbacks: &mut dyn dvda_author::Callbacks,
    ) -> Result<(), String> {
        #[cfg(feature = "direct-bridges")]
        {
            status(
                unsafe {
                    dvda_bridges::menu_media::dvda_menu_create_mpg(
                        path(y4m)?.as_ptr(),
                        path(wav)?.as_ptr(),
                        path(output)?.as_ptr(),
                        string(norm)?.as_ptr(),
                        string(aspect)?.as_ptr(),
                        still as i32,
                    )
                },
                callbacks,
            )
        }
        #[cfg(not(feature = "direct-bridges"))]
        {
            let _ = (y4m, wav, output, norm, aspect, still, callbacks);
            Err("Menu authoring requires the direct-bridges feature".into())
        }
    }
    fn subpictures(
        &mut self,
        xml: &Path,
        input: &Path,
        output: &Path,
        callbacks: &mut dyn dvda_author::Callbacks,
    ) -> Result<(), String> {
        #[cfg(feature = "direct-bridges")]
        {
            status(
                unsafe {
                    dvda_bridges::menu::dvda_menu_subpictures(
                        path(xml)?.as_ptr(),
                        path(input)?.as_ptr(),
                        path(output)?.as_ptr(),
                    )
                },
                callbacks,
            )
        }
        #[cfg(not(feature = "direct-bridges"))]
        {
            let _ = (xml, input, output, callbacks);
            Err("Menu authoring requires the direct-bridges feature".into())
        }
    }
    fn navigation(
        &mut self,
        xml: &Path,
        output: &Path,
        callbacks: &mut dyn dvda_author::Callbacks,
    ) -> Result<(), String> {
        #[cfg(feature = "direct-bridges")]
        {
            status(
                unsafe {
                    dvda_bridges::menu::dvda_menu_navigation(
                        path(xml)?.as_ptr(),
                        path(output)?.as_ptr(),
                    )
                },
                callbacks,
            )
        }
        #[cfg(not(feature = "direct-bridges"))]
        {
            let _ = (xml, output, callbacks);
            Err("Menu authoring requires the direct-bridges feature".into())
        }
    }
}
