use dvda_bridges::{
    Cancel, Emit,
    media::{Request, dvdamedia_run},
};
use std::ffi::{CString, c_void};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
unsafe extern "C" fn collect(state: *mut c_void, stream: i32, text: *const i8) {
    unsafe {
        (*(state.cast::<Vec<(i32, String)>>())).push((
            stream,
            std::ffi::CStr::from_ptr(text)
                .to_string_lossy()
                .into_owned(),
        ));
    }
}
#[derive(Default)]
struct Progress {
    messages: Vec<(i32, String)>,
    polls: usize,
}
unsafe extern "C" fn progress(state: *mut c_void, stream: i32, text: *const i8) {
    unsafe {
        collect(
            (&mut (*state.cast::<Progress>()).messages as *mut Vec<_>).cast(),
            stream,
            text,
        )
    }
}
unsafe extern "C" fn cancel_after_progress(state: *mut c_void) -> i32 {
    let state = unsafe { &mut *state.cast::<Progress>() };
    state.polls += 1;
    i32::from(
        state
            .messages
            .iter()
            .any(|(stream, text)| *stream == 1 && text.contains(',')),
    )
}
unsafe extern "C" fn cancelled(_: *mut c_void) -> i32 {
    1
}
type Run = unsafe extern "C" fn(*const Request, Emit, Cancel, *mut c_void) -> i32;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "legacy DLL and input required");
    let input = CString::new(args[2].clone()).unwrap();
    let path: Vec<_> = args[1].encode_utf16().chain([0]).collect();
    unsafe {
        let module = LoadLibraryW(path.as_ptr());
        assert!(!module.is_null(), "oracle load failure");
        let address = GetProcAddress(module, c"dvdamedia_run".as_ptr());
        assert!(!address.is_null());
        let oracle: Run = std::mem::transmute(address);
        if let Ok(destination) = std::env::var("DVDA_BRIDGE_COVER_FIXTURE") {
            let output = CString::new(destination.clone()).unwrap();
            let request = Request {
                size: std::mem::size_of::<Request>() as u32,
                abi: 1,
                operation: 3,
                rate: 0,
                bits: 16,
                output_format: 6,
                soxr: 0,
                compression: 0,
                cover: 0,
                tag_count: 0,
                input: input.as_ptr(),
                output: output.as_ptr(),
                tags: std::ptr::null(),
            };
            let mut messages = Vec::<(i32, String)>::new();
            assert_eq!(
                oracle(
                    &request,
                    Some(collect),
                    None,
                    (&mut messages as *mut Vec<_>).cast()
                ),
                0
            );
            let mut flac = std::fs::read(&destination).unwrap();
            assert_eq!(&flac[..4], b"fLaC");
            let mut end = 4;
            loop {
                let last = flac[end] & 128 != 0;
                let size = ((flac[end + 1] as usize) << 16)
                    | ((flac[end + 2] as usize) << 8)
                    | flac[end + 3] as usize;
                if last {
                    flac[end] &= 127;
                }
                end += 4 + size;
                if last {
                    break;
                }
            }
            let png = std::fs::read("build\\menu-matrix-pal-1\\图像.png").unwrap();
            let mut picture = Vec::new();
            picture.extend(3u32.to_be_bytes());
            picture.extend(9u32.to_be_bytes());
            picture.extend(b"image/png");
            picture.extend(0u32.to_be_bytes());
            for value in [720u32, 576, 24, 0, png.len() as u32] {
                picture.extend(value.to_be_bytes());
            }
            picture.extend(png);
            let mut block = vec![0x86];
            block.extend(&(picture.len() as u32).to_be_bytes()[1..]);
            block.extend(picture);
            flac.splice(end..end, block);
            std::fs::write(destination, flac).unwrap();
            println!("PASS generated attached-picture FLAC fixture with frozen C encoder");
            FreeLibrary(module);
            return;
        }
        for (operation, output_format, bits, rate) in [
            (1, 0, 0, 0),
            (2, 0, 0, 0),
            (3, 0, 0, 0),
            (3, 2, 24, 0),
            (3, 3, 16, 0),
            (3, 4, 20, 0),
            (3, 5, 16, 0),
            (3, 4, 24, 44100),
            (3, 1, 16, 0),
            (3, 1, 24, 44100),
            (3, 6, 16, 0),
            (3, 6, 20, 44100),
            (3, 1, 16, 48000),
            (3, 6, 24, 96000),
            (3, 6, 16, 32000),
            (3, 6, 16, 123),
            (4, 0, 0, 0),
            (5, 0, 0, 0),
        ] {
            let expected_path = CString::new("build\\bridge-media-c.pcm").unwrap();
            let actual_path = CString::new("build\\bridge-media-rust.pcm").unwrap();
            let tags = [c"title".as_ptr(), c"Rust bridge parity".as_ptr()];
            let mut request = Request {
                size: std::mem::size_of::<Request>() as u32,
                abi: 1,
                operation,
                rate,
                bits,
                output_format,
                soxr: u32::from(rate == 48000),
                compression: if rate == 96000 { 8 } else { 0 },
                cover: u32::from(output_format == 6),
                tag_count: u32::from(matches!(output_format, 1 | 6)),
                input: input.as_ptr(),
                output: expected_path.as_ptr(),
                tags: tags.as_ptr(),
            };
            let mut expected = Vec::<(i32, String)>::new();
            let mut actual = Vec::<(i32, String)>::new();
            let c = oracle(
                &request,
                Some(collect),
                None,
                (&mut expected as *mut Vec<(i32, String)>).cast(),
            );
            request.output = actual_path.as_ptr();
            let rust = dvdamedia_run(
                &request,
                Some(collect),
                None,
                (&mut actual as *mut Vec<(i32, String)>).cast(),
            );
            assert_eq!(rust, c, "status operation {operation}");
            assert_eq!(actual, expected, "output operation {operation}");
            if ((operation == 3 && matches!(output_format, 1 | 2 | 3 | 4 | 6))
                || matches!(operation, 4 | 5))
                && c == 0
            {
                assert_eq!(
                    std::fs::read("build\\bridge-media-c.pcm").unwrap(),
                    std::fs::read("build\\bridge-media-rust.pcm").unwrap(),
                    "PCM format {output_format}, bits {bits}, rate {rate}"
                );
            }
            println!(
                "PASS operation {operation}, format {output_format}, bits {bits}, rate {rate}: {} messages",
                actual.len()
            );
        }
        for (input_path, output_path, cancel) in [
            (args[2].as_str(), "build\\bridge-media-errors.pcm", true),
            (
                "build\\missing-bridge-input.mlp",
                "build\\bridge-media-errors.pcm",
                false,
            ),
            (
                args[2].as_str(),
                "build\\missing-bridge-directory\\output.pcm",
                false,
            ),
        ] {
            let input_path = CString::new(input_path).unwrap();
            let output_path = CString::new(output_path).unwrap();
            let request = Request {
                size: std::mem::size_of::<Request>() as u32,
                abi: 1,
                operation: 3,
                rate: 0,
                bits: 16,
                output_format: 2,
                soxr: 0,
                compression: 0,
                cover: 0,
                tag_count: 0,
                input: input_path.as_ptr(),
                output: output_path.as_ptr(),
                tags: std::ptr::null(),
            };
            let (mut expected, mut actual) =
                (Vec::<(i32, String)>::new(), Vec::<(i32, String)>::new());
            let callback: Cancel = if cancel { Some(cancelled) } else { None };
            let c = oracle(
                &request,
                Some(collect),
                callback,
                (&mut expected as *mut Vec<_>).cast(),
            );
            let r = dvdamedia_run(
                &request,
                Some(collect),
                callback,
                (&mut actual as *mut Vec<_>).cast(),
            );
            assert_eq!(r, c, "negative status");
            assert_eq!(actual, expected, "negative callbacks");
            println!("PASS negative/cancellation status {r}");
        }
        let output = CString::new("build\\bridge-media-invalid.pcm").unwrap();
        let base = Request {
            size: std::mem::size_of::<Request>() as u32,
            abi: 1,
            operation: 1,
            rate: 0,
            bits: 0,
            output_format: 0,
            soxr: 0,
            compression: 0,
            cover: 0,
            tag_count: 0,
            input: input.as_ptr(),
            output: output.as_ptr(),
            tags: std::ptr::null(),
        };
        let mut packet_request = base;
        packet_request.operation = 2;
        let (mut expected_progress, mut actual_progress) =
            (Progress::default(), Progress::default());
        let c = oracle(
            &packet_request,
            Some(progress),
            Some(cancel_after_progress),
            (&mut expected_progress as *mut Progress).cast(),
        );
        let rust = dvdamedia_run(
            &packet_request,
            Some(progress),
            Some(cancel_after_progress),
            (&mut actual_progress as *mut Progress).cast(),
        );
        assert_eq!(rust, c, "mid-operation cancellation status");
        assert_eq!(
            actual_progress.messages, expected_progress.messages,
            "mid-operation cancellation callbacks"
        );
        assert_eq!(
            rust, -1414092869,
            "must cancel after a real packet callback"
        );
        assert!(actual_progress.polls > 1);
        println!("PASS mid-operation cancellation after packet delivery: {rust}");
        for (label, request, with_emit) in [
            ("null request", std::ptr::null(), true),
            ("null emit", &base as *const Request, false),
        ] {
            let emit = if with_emit {
                Some(collect as unsafe extern "C" fn(_, _, _))
            } else {
                None
            };
            let c = oracle(request, emit, None, std::ptr::null_mut());
            let rust = dvdamedia_run(request, emit, None, std::ptr::null_mut());
            assert_eq!(rust, c, "{label}");
            println!("PASS malformed ABI {label}: {rust}");
        }
        for (label, change) in [
            ("small size", 0),
            ("wrong ABI", 1),
            ("null input", 2),
            ("invalid operation", 3),
        ] {
            let mut request = base;
            match change {
                0 => request.size -= 1,
                1 => request.abi += 1,
                2 => request.input = std::ptr::null(),
                _ => request.operation = 99,
            }
            let (mut expected, mut actual) =
                (Vec::<(i32, String)>::new(), Vec::<(i32, String)>::new());
            let c = oracle(
                &request,
                Some(collect),
                None,
                (&mut expected as *mut Vec<_>).cast(),
            );
            let rust = dvdamedia_run(
                &request,
                Some(collect),
                None,
                (&mut actual as *mut Vec<_>).cast(),
            );
            assert_eq!(rust, c, "{label}");
            assert_eq!(actual, expected, "{label} callbacks");
            println!("PASS malformed ABI {label}: {rust}");
        }
        for path in ["build\\bridge-media-c.pcm", "build\\bridge-media-rust.pcm"] {
            let _ = std::fs::remove_file(path);
        }
        FreeLibrary(module);
    }
}
