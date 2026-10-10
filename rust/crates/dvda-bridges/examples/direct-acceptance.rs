use dvda_bridges::{Call, direct::*};
use std::ffi::{CStr, CString, c_void};
unsafe extern "C" fn emit(state: *mut c_void, _: i32, text: *const i8) {
    unsafe {
        (*(state as *mut Vec<String>)).push(CStr::from_ptr(text).to_string_lossy().into_owned());
    }
}
unsafe extern "C" fn cancel(_: *mut c_void) -> i32 {
    1
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "audio and Unicode image fixtures");
    std::fs::copy(&args[1], "build\\桥接音频.wav").unwrap();
    let image_path = CString::new(args[2].as_str()).unwrap();
    let image = read_rgba(&image_path).unwrap();
    assert_eq!(
        image.pixels.len(),
        image.width as usize * image.height as usize * 4
    );
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let path = image_path.clone();
            let expected = image.clone();
            std::thread::spawn(move || {
                assert_eq!(read_rgba(&path).unwrap(), expected);
                let request = MediaRequest::new(1, "build\\桥接音频.wav", "").unwrap();
                let mut messages = Vec::<String>::new();
                let status = unsafe {
                    request.run_with_callbacks(Call {
                        emit: Some(emit),
                        cancel: None,
                        state: (&mut messages as *mut Vec<String>).cast(),
                    })
                };
                assert_eq!(status, 0);
                assert!(!messages.is_empty());
                messages
            })
        })
        .collect();
    let mut previous = None;
    for thread in threads {
        let messages = thread.join().unwrap();
        if let Some(expected) = &previous {
            assert_eq!(&messages, expected);
        }
        previous = Some(messages);
    }
    let mut request =
        MediaRequest::new(3, "build\\桥接音频.wav", "build\\direct-cancel.wav").unwrap();
    request.bits = 16;
    assert_eq!(
        unsafe {
            request.run_with_callbacks(Call {
                emit: None,
                cancel: Some(cancel),
                state: std::ptr::null_mut(),
            })
        },
        -1414092869
    );
    let arguments = vec![CString::new("identify").unwrap(), image_path];
    assert_eq!(
        unsafe {
            image_run_with_callbacks(
                &arguments,
                Call {
                    emit: None,
                    cancel: Some(cancel),
                    state: std::ptr::null_mut(),
                },
            )
        },
        130
    );
    assert_eq!(image_run(&[]), 2);
    assert!(read_rgba(c"build\\missing-direct.png").is_err());
    let mut messages = Vec::<String>::new();
    assert_eq!(
        unsafe {
            image_run_with_callbacks(
                &arguments,
                Call {
                    emit: Some(emit),
                    cancel: None,
                    state: (&mut messages as *mut Vec<String>).cast(),
                },
            )
        },
        0
    );
    // Identify without -format writes no metadata callback in the legacy API.
    assert!(messages.is_empty());
    std::fs::remove_file("build\\桥接音频.wav").unwrap();
    let _ = std::fs::remove_file("build\\direct-cancel.wav");
    println!(
        "PASS direct rlib Unicode, four-way concurrent image/media callbacks, cancellation, errors and buffer sizing"
    );
}
