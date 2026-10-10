//! Exercise real, statically linked vendor calls through the public Rust boundary.
#[cfg(feature = "direct-link")]
mod acceptance {
    use dvda_menu::{
        MenuRequest, dvda_menu_run_navigation, dvda_menu_run_spu, run_navigation, run_spu,
    };
    use std::{
        ffi::{CStr, CString, c_char, c_void},
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicBool, AtomicI32, Ordering},
    };

    static FAIL_PIXELS: AtomicBool = AtomicBool::new(false);
    static REENTER: AtomicBool = AtomicBool::new(false);
    static REENTERED: AtomicBool = AtomicBool::new(false);
    static NESTED_STATUS: AtomicI32 = AtomicI32::new(0);
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessHandleCount(process: *mut c_void, count: *mut u32) -> i32;
        fn GetProcessHeaps(count: u32, heaps: *mut *mut c_void) -> u32;
    }
    fn handles() -> u32 {
        let mut count = 0;
        assert_ne!(
            unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) },
            0
        );
        count
    }
    fn heaps() -> u32 {
        unsafe { GetProcessHeaps(0, std::ptr::null_mut()) }
    }
    fn c(path: &Path) -> CString {
        CString::new(path.to_str().unwrap()).unwrap()
    }
    fn xml(path: &Path) -> String {
        path.to_str()
            .unwrap()
            .replace('&', "&amp;")
            .replace('"', "&quot;")
    }
    unsafe extern "C" fn pixels(
        path: *const c_char,
        output: *mut u8,
        capacity: usize,
        width: *mut u32,
        height: *mut u32,
    ) -> i32 {
        std::panic::catch_unwind(|| unsafe {
            if FAIL_PIXELS.load(Ordering::Relaxed) {
                return 1;
            }
            if REENTER.load(Ordering::Relaxed) && !REENTERED.swap(true, Ordering::Relaxed) {
                // The same-thread public entry must refuse reentry before taking
                // its process lock. It must return without entering a C core.
                NESTED_STATUS.store(
                    dvda_menu_run_navigation(std::ptr::null()),
                    Ordering::Relaxed,
                );
            }
            if path.is_null() || width.is_null() || height.is_null() {
                return 1;
            }
            let path = CStr::from_ptr(path).to_str().unwrap();
            *width = 720;
            *height = 576;
            if output.is_null() {
                return 0;
            }
            if capacity < 720 * 576 * 4 {
                return 1;
            }
            let color = if path.ends_with("highlight.png") {
                [255, 0, 0, 255]
            } else if path.ends_with("select.png") {
                [0, 255, 0, 255]
            } else {
                [255, 255, 255, 255]
            };
            for (position, pixel) in std::slice::from_raw_parts_mut(output, 720 * 576 * 4)
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .enumerate()
            {
                let (x, y) = (position % 720, position / 720);
                *pixel = if (20..700).contains(&x) && (56..100).contains(&y) {
                    color
                } else {
                    [0; 4]
                };
            }
            0
        })
        .unwrap_or(1)
    }
    fn spu(document: &Path, input: &Path, output: &Path) -> i32 {
        unsafe { run_spu(&c(document), &c(input), &c(output), Some(pixels)) }
            .map_or_else(|error| error.0, |()| 0)
    }
    fn nav(document: &Path, output: &Path) -> i32 {
        run_navigation(&c(document), &c(output)).map_or_else(|error| error.0, |()| 0)
    }
    fn tree(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn scan(root: &Path, path: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    scan(root, &path, result);
                } else {
                    result.push((
                        path.strip_prefix(root).unwrap().to_owned(),
                        fs::read(path).unwrap(),
                    ));
                }
            }
        }
        let mut result = Vec::new();
        scan(directory, directory, &mut result);
        result.sort();
        result
    }
    fn rename_roundtrip(path: &Path) {
        let swap = path.with_extension("resource-close-test");
        fs::rename(path, &swap).unwrap();
        fs::rename(swap, path).unwrap();
    }
    pub fn run() {
        let arguments: Vec<_> = std::env::args_os().skip(1).collect();
        assert_eq!(arguments.len(), 2, "session-acceptance OUTPUT SOURCE_MPEG");
        let root = PathBuf::from(&arguments[0]);
        fs::create_dir_all(&root).unwrap();
        let input = root.join("背景 日本語.mpg");
        fs::copy(&arguments[1], &input).unwrap();
        let output = root.join("menu.mpg");
        let document = root.join("菜单.xml");
        fs::write(&document, format!("<subpictures format=\"PAL\"><stream><spu force=\"yes\" start=\"00:00:00.00\" image=\"{}\" highlight=\"{}\" select=\"{}\"><button name=\"button01\" x0=\"20\" y0=\"56\" x1=\"700\" y1=\"100\"/></spu></stream></subpictures>", xml(&root.join("image.png")), xml(&root.join("highlight.png")), xml(&root.join("select.png")))).unwrap();
        let nav_document = root.join("导航.xml");
        let nav_xml = format!(
            "<dvdauthor jumppad=\"1\"><amgm><menus><video format=\"pal\"/><audio format=\"mp2\" lang=\"en\"/><pgc><pre>g0=1;</pre><button name=\"button01\">jump menu 1;</button><vob file=\"{}\"/><post>jump menu 1;</post></pgc></menus></amgm></dvdauthor>",
            xml(&output)
        );
        fs::write(&nav_document, &nav_xml).unwrap();
        let nav_output = root.join("导航 日本語");
        fs::create_dir(&nav_output).unwrap();
        fs::create_dir(nav_output.join("AUDIO_TS")).unwrap();
        fs::create_dir(nav_output.join("VIDEO_TS")).unwrap();
        assert_eq!(spu(&document, &input, &output), 0);
        let expected_spu = fs::read(&output).unwrap();
        assert!(!expected_spu.is_empty());
        assert_eq!(nav(&nav_document, &nav_output), 0);
        let expected_nav = tree(&nav_output);
        assert!(!expected_nav.is_empty());
        fs::write(root.join("expected-spu.mpg"), &expected_spu).unwrap();
        let before_handles = handles();
        let before_heaps = heaps();
        let bad = root.join("bad.xml");
        let invalid_vm = root.join("invalid-vm.xml");
        fs::write(&invalid_vm, nav_xml.replace("g0=1;", "not_a_vm_command;")).unwrap();
        let bad_boolean_nav = root.join("invalid-nav-boolean.xml");
        fs::write(&bad_boolean_nav, "<dvdauthor jumppad=\"invalid\"/>").unwrap();
        let mut failures = 0;
        for _ in 0..12 {
            for content in [
                "<subpictures>",
                "<subpictures><stream><unknown/></stream></subpictures>",
                "<subpictures><stream><spu force=\"invalid\"/></stream></subpictures>",
                "<!DOCTYPE subpictures><subpictures/>",
                "<subpictures>&#1;</subpictures>",
            ] {
                fs::write(&bad, content).unwrap();
                assert_ne!(spu(&bad, &input, &output), 0);
                failures += 1;
                assert_eq!(spu(&document, &input, &output), 0);
                assert_eq!(fs::read(&output).unwrap(), expected_spu);
            }
            fs::write(&bad, b"<subpictures>\xff</subpictures>").unwrap();
            assert_ne!(spu(&bad, &input, &output), 0);
            failures += 1;
            assert_ne!(spu(&root.join("missing.xml"), &input, &output), 0);
            failures += 1;
            assert_ne!(spu(&document, &root.join("missing.mpg"), &output), 0);
            failures += 1;
            assert_ne!(spu(&document, &input, &root.join("absent/failed.mpg")), 0);
            failures += 1;
            assert_ne!(nav(&invalid_vm, &nav_output), 0);
            failures += 1;
            assert_ne!(nav(&bad_boolean_nav, &nav_output), 0);
            failures += 1;
            assert_ne!(nav(&root.join("absent-nav.xml"), &nav_output), 0);
            failures += 1;
            assert_eq!(spu(&document, &input, &output), 0);
            assert_eq!(fs::read(&output).unwrap(), expected_spu);
            assert_eq!(nav(&nav_document, &nav_output), 0);
            assert_eq!(tree(&nav_output), expected_nav);
            rename_roundtrip(&input);
            rename_roundtrip(&output);
        }
        FAIL_PIXELS.store(true, Ordering::Relaxed);
        assert_ne!(spu(&document, &input, &output), 0);
        failures += 1;
        FAIL_PIXELS.store(false, Ordering::Relaxed);
        assert_eq!(spu(&document, &input, &output), 0);
        assert_eq!(fs::read(&output).unwrap(), expected_spu);
        unsafe {
            assert_ne!(dvda_menu_run_spu(std::ptr::null()), 0);
            assert_ne!(dvda_menu_run_navigation(std::ptr::null()), 0);
            let xml = c(&document);
            let input = c(&input);
            let output = c(&output);
            let mut request = MenuRequest {
                size: 0,
                xml: xml.as_ptr(),
                input: input.as_ptr(),
                output: output.as_ptr(),
                read_rgba: Some(pixels),
            };
            assert_ne!(dvda_menu_run_spu(&request), 0);
            request.size = std::mem::size_of::<MenuRequest>() as u32;
            for field in 0..3 {
                let saved = match field {
                    0 => request.xml,
                    1 => request.input,
                    _ => request.output,
                };
                match field {
                    0 => request.xml = std::ptr::null(),
                    1 => request.input = std::ptr::null(),
                    _ => request.output = std::ptr::null(),
                };
                assert_ne!(dvda_menu_run_spu(&request), 0);
                match field {
                    0 => request.xml = saved,
                    1 => request.input = saved,
                    _ => request.output = saved,
                };
            }
            let invalid = CString::new(vec![0xff]).unwrap();
            request.xml = invalid.as_ptr();
            assert_ne!(dvda_menu_run_spu(&request), 0);
            request.xml = xml.as_ptr();
            request.input = invalid.as_ptr();
            assert_ne!(dvda_menu_run_spu(&request), 0);
            request.input = input.as_ptr();
            request.output = invalid.as_ptr();
            assert_ne!(dvda_menu_run_spu(&request), 0);
        }
        assert_eq!(spu(&document, &input, &output), 0);
        REENTER.store(true, Ordering::Relaxed);
        assert_eq!(spu(&document, &input, &output), 0);
        REENTER.store(false, Ordering::Relaxed);
        assert!(REENTERED.load(Ordering::Relaxed));
        assert_ne!(NESTED_STATUS.load(Ordering::Relaxed), 0);
        assert_eq!(fs::read(&output).unwrap(), expected_spu);
        let jobs: Vec<_> = (0..4)
            .map(|rank| {
                let document = document.clone();
                let input = input.clone();
                let output = root.join(format!("thread-{rank}.mpg"));
                std::thread::spawn(move || {
                    assert_eq!(spu(&document, &input, &output), 0);
                    fs::read(output).unwrap()
                })
            })
            .collect();
        for job in jobs {
            assert_eq!(job.join().unwrap(), expected_spu);
        }
        assert_eq!(nav(&nav_document, &nav_output), 0);
        assert_eq!(tree(&nav_output), expected_nav);
        let after_handles = handles();
        let after_heaps = heaps();
        assert!(
            after_handles <= before_handles + 2,
            "Windows handles leaked: {before_handles} -> {after_handles}"
        );
        assert!(
            after_heaps <= before_heaps,
            "Private heaps leaked: {before_heaps} -> {after_heaps}"
        );
        fs::write(root.join("session-report.json"), format!("{{\"passed\":true,\"failure_calls\":{failures},\"reentry_rejected\":true,\"parallel_calls\":4,\"handles_before\":{before_handles},\"handles_after\":{after_handles},\"heaps_before\":{before_heaps},\"heaps_after\":{after_heaps},\"spu_bytes\":{}}}\n", expected_spu.len())).unwrap();
        println!("Session acceptance passed: {}", root.display());
    }
}
#[cfg(feature = "direct-link")]
fn main() {
    acceptance::run();
}
#[cfg(not(feature = "direct-link"))]
fn main() {
    panic!("Enable the direct-link feature for this acceptance example");
}
