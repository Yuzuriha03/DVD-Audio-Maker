use dvda_bridges::menu_media::dvda_menu_create_mpg;
use std::ffi::{CString, c_void};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    unsafe {
        let module = LoadLibraryW(
            args[1]
                .encode_utf16()
                .chain([0])
                .collect::<Vec<_>>()
                .as_ptr(),
        );
        assert!(!module.is_null());
        let run: unsafe extern "C" fn(
            *const i8,
            *const i8,
            *const i8,
            *const i8,
            *const i8,
            i32,
        ) -> i32 = std::mem::transmute(GetProcAddress(module, c"dvda_menu_create_mpg".as_ptr()));
        for norm in ["pal", "ntsc"] {
            let height = if norm == "pal" { 576 } else { 480 };
            let mut bytes = format!(
                "YUV4MPEG2 W720 H{height} F{} Ip A4:3 C420jpeg\nFRAME\n",
                if norm == "pal" { "25:1" } else { "30000:1001" }
            )
            .into_bytes();
            bytes.extend(vec![64u8; 720 * height]);
            bytes.extend(vec![128u8; 720 * height / 2]);
            std::fs::write("build\\bridge-menu.y4m", bytes).unwrap();
            let norm = CString::new(norm).unwrap();
            for count in [0usize, 1, 1151, 1152, 1153, 12000] {
                let wav = if count == 0 {
                    std::ptr::null()
                } else {
                    let size = (count * 4) as u32;
                    let mut data = Vec::new();
                    data.extend(b"RIFF");
                    data.extend((size + 36).to_le_bytes());
                    data.extend(b"WAVEfmt ");
                    data.extend(16u32.to_le_bytes());
                    data.extend(1u16.to_le_bytes());
                    data.extend(2u16.to_le_bytes());
                    data.extend(48000u32.to_le_bytes());
                    data.extend(192000u32.to_le_bytes());
                    data.extend(4u16.to_le_bytes());
                    data.extend(16u16.to_le_bytes());
                    data.extend(b"data");
                    data.extend(size.to_le_bytes());
                    for n in 0..count * 2 {
                        data.extend(((n as i32 * 151 % 65536 - 32768) as i16).to_le_bytes());
                    }
                    std::fs::write("build\\bridge-menu.wav", data).unwrap();
                    c"build\\bridge-menu.wav".as_ptr()
                };
                for aspect in [c"1:1", c"4:3", c"16:9", c"2.21:1"] {
                    for still in [0, 1] {
                        let c = run(
                            c"build\\bridge-menu.y4m".as_ptr(),
                            wav,
                            c"build\\bridge-menu-c.mpg".as_ptr(),
                            norm.as_ptr(),
                            aspect.as_ptr(),
                            still,
                        );
                        let r = dvda_menu_create_mpg(
                            c"build\\bridge-menu.y4m".as_ptr(),
                            wav,
                            c"build\\bridge-menu-rust.mpg".as_ptr(),
                            norm.as_ptr(),
                            aspect.as_ptr(),
                            still,
                        );
                        assert_eq!(r, c);
                        assert_eq!(r, 0);
                        assert_eq!(
                            std::fs::read("build\\bridge-menu-c.mpg").unwrap(),
                            std::fs::read("build\\bridge-menu-rust.mpg").unwrap()
                        );
                        println!(
                            "PASS {:?} still={still} samples={count} aspect={aspect:?}",
                            norm
                        );
                    }
                }
            }
        }
        let mut negative_failures = 0;
        for (y4m, wav, output, norm, aspect) in [
            (
                c"build\\missing.y4m",
                std::ptr::null(),
                c"build\\bridge-menu-error.mpg",
                c"pal",
                c"4:3",
            ),
            (
                c"build\\bridge-menu.y4m",
                c"build\\missing.wav".as_ptr(),
                c"build\\bridge-menu-error.mpg",
                c"ntsc",
                c"4:3",
            ),
            (
                c"build\\bridge-menu.y4m",
                std::ptr::null(),
                c"build\\missing-bridge-directory\\menu.mpg",
                c"ntsc",
                c"4:3",
            ),
            (
                c"build\\bridge-menu.y4m",
                std::ptr::null(),
                c"build\\bridge-menu-error.mpg",
                c"invalid",
                c"4:3",
            ),
            (
                c"build\\bridge-menu.y4m",
                std::ptr::null(),
                c"build\\bridge-menu-error.mpg",
                c"ntsc",
                c"invalid",
            ),
        ] {
            let c = run(
                y4m.as_ptr(),
                wav,
                output.as_ptr(),
                norm.as_ptr(),
                aspect.as_ptr(),
                0,
            );
            let r = dvda_menu_create_mpg(
                y4m.as_ptr(),
                wav,
                output.as_ptr(),
                norm.as_ptr(),
                aspect.as_ptr(),
                0,
            );
            println!("negative {y4m:?} {wav:?} {output:?} {norm:?} {aspect:?}: Rust={r}, C={c}");
            negative_failures += usize::from(r != c);
        }
        std::fs::write("build\\bridge-menu.y4m", b"not a Y4M image").unwrap();
        let c = run(
            c"build\\bridge-menu.y4m".as_ptr(),
            std::ptr::null(),
            c"build\\bridge-menu-error.mpg".as_ptr(),
            c"pal".as_ptr(),
            c"4:3".as_ptr(),
            0,
        );
        let r = dvda_menu_create_mpg(
            c"build\\bridge-menu.y4m".as_ptr(),
            std::ptr::null(),
            c"build\\bridge-menu-error.mpg".as_ptr(),
            c"pal".as_ptr(),
            c"4:3".as_ptr(),
            0,
        );
        println!("malformed menu status: Rust={r}, C={c}");
        negative_failures += usize::from(r != c);
        assert_eq!(negative_failures, 0, "negative menu parity");
        for path in [
            "build\\bridge-menu-error.mpg",
            "build\\bridge-menu.wav",
            "build\\bridge-menu.y4m",
            "build\\bridge-menu-c.mpg",
            "build\\bridge-menu-rust.mpg",
        ] {
            let _ = std::fs::remove_file(path);
        }
    }
}
