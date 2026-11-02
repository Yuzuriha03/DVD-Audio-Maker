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
    fn write_y4m(
        &mut self,
        input: &Path,
        output: &Path,
        norm: &str,
        aspect: &str,
        callbacks: &mut dyn dvda_author::Callbacks,
    ) -> Result<(), String> {
        if callbacks.cancelled() {
            return Err("Author cancelled".into());
        }
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let image =
                dvda_image::codec::decode(std::io::BufReader::new(std::fs::File::open(input)?))?;
            let size = image.dimensions();
            // The old RGB export ignores alpha rather than flattening it.
            let rgb: Vec<u8> = image
                .rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| pixel[..3].iter().copied())
                .collect();
            if callbacks.cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Author cancelled",
                ));
            }
            let mut bytes = Vec::new();
            dvda_image::y4m::write_rgb(
                &mut bytes,
                &rgb,
                size.width() as usize,
                size.height() as usize,
                if norm == "ntsc" { "30" } else { "25" },
                aspect,
            )?;
            let mut file = std::fs::File::create(output)?;
            let written = (|| -> std::io::Result<()> {
                file.write_all(&bytes)?;
                file.flush()?;
                file.sync_all()?;
                if callbacks.cancelled() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Author cancelled",
                    ));
                }
                Ok(())
            })();
            drop(file);
            if written.is_err() {
                let _ = std::fs::remove_file(output);
            }
            written
        })();
        result.map_err(|error| error.to_string())
    }
    fn create_mpg(
        &mut self,
        request: dvda_author::menu::MpegRequest<'_>,
        callbacks: &mut dyn dvda_author::Callbacks,
    ) -> Result<(), String> {
        let dvda_author::menu::MpegRequest {
            y4m,
            wav,
            output,
            norm,
            aspect,
            still,
        } = request;
        // DVD video has only NTSC and PAL formats; SECAM regions use PAL
        // (625-line, 25 fps) discs, which is how the author lays SECAM out.
        let norm = if norm == "ntsc" { "ntsc" } else { "pal" };
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

#[cfg(test)]
mod tests {
    use super::*;
    use dvda_author::menu::Backend as _;

    #[test]
    fn rust_y4m_preserves_rgb_and_cleans_cancelled_output() {
        struct Cancel {
            calls: usize,
            at: usize,
        }
        impl dvda_author::Callbacks for Cancel {
            fn emit(&mut self, _: i32, _: &str) {}
            fn cancelled(&mut self) -> bool {
                self.calls += 1;
                self.calls >= self.at
            }
        }
        let input = std::env::temp_dir().join(format!("dvda-y4m-{}.png", std::process::id()));
        let output = input.with_extension("y4m");
        let image = dvda_image::raster::Raster::solid(
            dvda_image::geometry::Dimensions::new(2, 2).unwrap(),
            [255, 0, 0, 0],
        )
        .unwrap();
        dvda_image::codec::write_png(&image, std::fs::File::create(&input).unwrap()).unwrap();
        for (norm, rate) in [("pal", "25"), ("ntsc", "30"), ("secam", "25")] {
            let mut callbacks = Cancel {
                calls: 0,
                at: usize::MAX,
            };
            Backend
                .write_y4m(&input, &output, norm, "4:3", &mut callbacks)
                .unwrap();
            let mut expected = Vec::new();
            dvda_image::y4m::write_rgb(&mut expected, &[255, 0, 0].repeat(4), 2, 2, rate, "4:3")
                .unwrap();
            assert_eq!(std::fs::read(&output).unwrap(), expected);
            std::fs::remove_file(&output).unwrap();
        }
        let mut callbacks = Cancel { calls: 0, at: 3 };
        assert!(
            Backend
                .write_y4m(&input, &output, "pal", "4:3", &mut callbacks)
                .is_err()
        );
        assert!(!output.exists());
        let mut callbacks = Cancel {
            calls: 0,
            at: usize::MAX,
        };
        assert!(
            Backend
                .write_y4m(&input, &output, "pal", "invalid", &mut callbacks)
                .is_err()
        );
        assert!(!output.exists());
        std::fs::remove_file(input).unwrap();
    }
}
