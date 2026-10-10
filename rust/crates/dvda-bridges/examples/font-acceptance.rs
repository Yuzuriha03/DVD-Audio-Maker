use dvda_bridges::{Call, direct::image_run_with_callbacks};
use std::{
    ffi::{CStr, CString, c_void},
    path::PathBuf,
};

unsafe extern "C" fn emit(state: *mut c_void, stream: i32, text: *const i8) {
    let (output, errors) = unsafe { &mut *state.cast::<(String, String)>() };
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    if stream == 1 {
        output.push_str(&text);
    } else {
        errors.push_str(&text);
    }
}

fn main() {
    let runtime = PathBuf::from(std::env::args_os().nth(1).expect("image runtime directory"));
    assert!(runtime.join("type.xml").is_file());
    assert!(runtime.join("fonts\\DvdaNotoCJK-Regular.ttc").is_file());
    assert_ne!(runtime, std::env::current_exe().unwrap().parent().unwrap());
    // Startup precedes native initialization and thread creation.
    unsafe { std::env::set_var("DVDA_IMAGE_NATIVE_DIR", &runtime) };
    for face in ["SC", "JP", "KR"] {
        for text in ["Ag(", "中文", "あいう", "アイウ", "한글", "、《》"] {
            let arguments: Vec<_> = [
                "magick".to_owned(),
                "-size".into(),
                "160x48".into(),
                "xc:none".into(),
                "-font".into(),
                format!("DVDA-Noto-Sans-CJK-{face}"),
                "-pointsize".into(),
                "20".into(),
                "-fill".into(),
                "white".into(),
                "-annotate".into(),
                "+2+32".into(),
                text.into(),
                "-format".into(),
                "%[fx:mean.a]".into(),
                "info:".into(),
            ]
            .into_iter()
            .map(|argument| CString::new(argument).unwrap())
            .collect();
            let mut messages = (String::new(), String::new());
            let status = unsafe {
                image_run_with_callbacks(
                    &arguments,
                    Call {
                        emit: Some(emit),
                        cancel: None,
                        state: (&mut messages as *mut (String, String)).cast(),
                    },
                )
            };
            assert_eq!(status, 0, "{face} {text}: {}", messages.1);
            assert!(messages.1.is_empty(), "{face} {text}: {}", messages.1);
            let ink: f64 = messages.0.trim().parse().expect("rendered alpha mean");
            assert!(ink.is_finite() && ink > 0.0, "{face} {text}: no ink");
        }
    }
    println!(
        "PASS 18 directly linked CJK font renders from runtime configuration outside EXE directory"
    );
}
