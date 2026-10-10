use dvda_bridges::{
    Cancel, Emit,
    image::{dvda_image_read_rgba, dvda_image_run, dvda_image_write_y4m},
};
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
unsafe extern "C" fn cancelled(_: *mut c_void) -> i32 {
    1
}
type Run = unsafe extern "C" fn(i32, *const *const i8, Emit, Cancel, *mut c_void) -> i32;
use std::ffi::{CString, c_void};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(args.len() >= 3, "oracle DLL and image paths required");
    unsafe {
        let module = LoadLibraryW(
            args[1]
                .encode_utf16()
                .chain([0])
                .collect::<Vec<_>>()
                .as_ptr(),
        );
        assert!(!module.is_null());
        let rgba: unsafe extern "C" fn(*const i8, *mut u8, usize, *mut u32, *mut u32) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"dvda_image_read_rgba".as_ptr()));
        let y4m: unsafe extern "C" fn(*const i8, *const i8, *const i8, *const i8) -> i32 =
            std::mem::transmute(GetProcAddress(module, c"dvda_image_write_y4m".as_ptr()));
        let run: Run = std::mem::transmute(GetProcAddress(module, c"dvda_image_run".as_ptr()));
        if std::env::var_os("DVDA_BRIDGE_IMAGE_FIXTURES").is_some() {
            for shape in ["3x3", "722x2", "2x2"] {
                let output = format!("build\\bridge-{shape}.png");
                let words = ["convert", "-size", shape, "xc:red", "-strip", &output];
                let owned: Vec<_> = words.iter().map(|s| CString::new(*s).unwrap()).collect();
                let pointers: Vec<_> = owned.iter().map(|s| s.as_ptr()).collect();
                assert_eq!(
                    run(
                        pointers.len() as i32,
                        pointers.as_ptr(),
                        None,
                        None,
                        std::ptr::null_mut()
                    ),
                    0
                );
            }
            FreeLibrary(module);
            return;
        }
        for (label, count, pointers) in [
            ("zero count", 0, std::ptr::null()),
            ("short count", 1, std::ptr::null()),
            ("excess count", 8193, std::ptr::null()),
            ("null array", 2, std::ptr::null()),
        ] {
            let (mut expected, mut actual) = (Vec::new(), Vec::new());
            let c = run(
                count,
                pointers,
                Some(collect),
                None,
                (&mut expected as *mut Vec<(i32, String)>).cast(),
            );
            let r = dvda_image_run(
                count,
                pointers,
                Some(collect),
                None,
                (&mut actual as *mut Vec<(i32, String)>).cast(),
            );
            assert_eq!(r, c, "malformed CLI status {label}");
            assert_eq!(actual, expected, "malformed CLI callbacks {label}");
        }
        let null_argument = [c"identify".as_ptr(), std::ptr::null()];
        let (mut expected, mut actual) = (Vec::new(), Vec::new());
        let c = run(
            2,
            null_argument.as_ptr(),
            Some(collect),
            None,
            (&mut expected as *mut Vec<(i32, String)>).cast(),
        );
        let r = dvda_image_run(
            2,
            null_argument.as_ptr(),
            Some(collect),
            None,
            (&mut actual as *mut Vec<(i32, String)>).cast(),
        );
        assert_eq!(r, c, "malformed CLI null argument status");
        assert_eq!(actual, expected, "malformed CLI null argument callbacks");
        println!("malformed CLI parity passed");
        for source in &args[2..] {
            for words in [
                vec!["identify", "-format", "%w %h %m", source.as_str()],
                vec!["convert", source.as_str(), "-format", "%w %h", "info:"],
                vec!["convert", "-list", "font"],
                vec!["unsupported", "x"],
            ] {
                let owned: Vec<_> = words.iter().map(|s| CString::new(*s).unwrap()).collect();
                let pointers: Vec<_> = owned.iter().map(|s| s.as_ptr()).collect();
                for cancel in [
                    None,
                    Some(cancelled as unsafe extern "C" fn(*mut c_void) -> i32),
                ] {
                    let (mut expected, mut actual) =
                        (Vec::<(i32, String)>::new(), Vec::<(i32, String)>::new());
                    let c = run(
                        pointers.len() as i32,
                        pointers.as_ptr(),
                        Some(collect),
                        cancel,
                        (&mut expected as *mut Vec<(i32, String)>).cast(),
                    );
                    let r = dvda_image_run(
                        pointers.len() as i32,
                        pointers.as_ptr(),
                        Some(collect),
                        cancel,
                        (&mut actual as *mut Vec<(i32, String)>).cast(),
                    );
                    assert_eq!(r, c, "CLI status {words:?}");
                    assert_eq!(actual, expected, "CLI output {words:?}");
                }
            }
            for (index, words) in [
                vec![
                    "convert",
                    source.as_str(),
                    "-resize",
                    "320x240!",
                    "-strip",
                    "build\\bridge-cli.png",
                ],
                vec![
                    "magick",
                    source.as_str(),
                    "-flop",
                    "-strip",
                    "build\\bridge-cli.png",
                ],
                vec![
                    "convert",
                    "-size",
                    "64x48",
                    "xc:red",
                    "-fill",
                    "blue",
                    "-draw",
                    "rectangle 4,4 20,20",
                    "-strip",
                    "build\\bridge-cli.png",
                ],
            ]
            .into_iter()
            .enumerate()
            {
                let owned: Vec<_> = words.iter().map(|s| CString::new(*s).unwrap()).collect();
                let pointers: Vec<_> = owned.iter().map(|s| s.as_ptr()).collect();
                let mut expected = Vec::<(i32, String)>::new();
                let mut actual = Vec::<(i32, String)>::new();
                let c = run(
                    pointers.len() as i32,
                    pointers.as_ptr(),
                    Some(collect),
                    None,
                    (&mut expected as *mut Vec<_>).cast(),
                );
                let cbytes = std::fs::read("build\\bridge-cli.png").ok();
                let _ = std::fs::remove_file("build\\bridge-cli.png");
                let r = dvda_image_run(
                    pointers.len() as i32,
                    pointers.as_ptr(),
                    Some(collect),
                    None,
                    (&mut actual as *mut Vec<_>).cast(),
                );
                assert_eq!(r, c, "file command status {index}");
                assert_eq!(actual, expected, "file command callback {index}");
                if c == 0 {
                    assert_eq!(
                        std::fs::read("build\\bridge-cli.png").ok(),
                        cbytes,
                        "file command bytes {index}"
                    );
                }
                let _ = std::fs::remove_file("build\\bridge-cli.png");
                println!("PASS CLI file/drawing command {index}: {source}");
            }
            let input = CString::new(source.as_str()).unwrap();
            for capacity in [0, 32, 720 * 576 * 4] {
                let mut expected = vec![0; capacity];
                let mut actual = vec![0; capacity];
                let (mut cw, mut ch, mut rw, mut rh) = (0, 0, 0, 0);
                let c = rgba(
                    input.as_ptr(),
                    expected.as_mut_ptr(),
                    capacity,
                    &mut cw,
                    &mut ch,
                );
                let r = dvda_image_read_rgba(
                    input.as_ptr(),
                    actual.as_mut_ptr(),
                    capacity,
                    &mut rw,
                    &mut rh,
                );
                assert_eq!((r, rw, rh), (c, cw, ch), "RGBA status {source}");
                assert_eq!(actual, expected, "RGBA bytes {source}");
            }
            for rate in ["25", "30"] {
                for aspect in ["1:1", "4:3", "16:9", "2.21:1"] {
                    let rate = CString::new(rate).unwrap();
                    let aspect = CString::new(aspect).unwrap();
                    let cpath = c"build\\bridge-image-c.y4m";
                    let rpath = c"build\\bridge-image-rust.y4m";
                    let c = y4m(
                        input.as_ptr(),
                        cpath.as_ptr(),
                        rate.as_ptr(),
                        aspect.as_ptr(),
                    );
                    let r = dvda_image_write_y4m(
                        input.as_ptr(),
                        rpath.as_ptr(),
                        rate.as_ptr(),
                        aspect.as_ptr(),
                    );
                    assert_eq!(r, c, "Y4M status {source}");
                    if c == 0 {
                        assert_eq!(
                            std::fs::read("build\\bridge-image-c.y4m").unwrap(),
                            std::fs::read("build\\bridge-image-rust.y4m").unwrap(),
                            "Y4M bytes {source}"
                        );
                    }
                }
            }
            println!("PASS RGBA capacities and eight Y4M profiles: {source}");
        }
        let input = CString::new(args[2].as_str()).unwrap();
        for missing in 0..4 {
            let mut expected = [91u8; 16];
            let mut actual = expected;
            let (mut cw, mut ch, mut rw, mut rh) = (77, 88, 77, 88);
            let source = if missing == 0 {
                std::ptr::null()
            } else {
                input.as_ptr()
            };
            let c = rgba(
                source,
                if missing == 1 {
                    std::ptr::null_mut()
                } else {
                    expected.as_mut_ptr()
                },
                16,
                if missing == 2 {
                    std::ptr::null_mut()
                } else {
                    &mut cw
                },
                if missing == 3 {
                    std::ptr::null_mut()
                } else {
                    &mut ch
                },
            );
            let r = dvda_image_read_rgba(
                source,
                if missing == 1 {
                    std::ptr::null_mut()
                } else {
                    actual.as_mut_ptr()
                },
                16,
                if missing == 2 {
                    std::ptr::null_mut()
                } else {
                    &mut rw
                },
                if missing == 3 {
                    std::ptr::null_mut()
                } else {
                    &mut rh
                },
            );
            assert_eq!((r, rw, rh, actual), (c, cw, ch, expected));
            println!("PASS null RGBA argument {missing}: {r}");
        }
        for missing in 0..4 {
            let mut pointers = [
                input.as_ptr(),
                c"build\\bridge-null.y4m".as_ptr(),
                c"25".as_ptr(),
                c"4:3".as_ptr(),
            ];
            pointers[missing] = std::ptr::null();
            let c = y4m(pointers[0], pointers[1], pointers[2], pointers[3]);
            let r = dvda_image_write_y4m(pointers[0], pointers[1], pointers[2], pointers[3]);
            assert_eq!(r, c);
            println!("PASS null Y4M argument {missing}: {r}");
        }
        for path in ["build\\bridge-image-c.y4m", "build\\bridge-image-rust.y4m"] {
            let _ = std::fs::remove_file(path);
        }
        FreeLibrary(module);
    }
}
