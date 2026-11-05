//! Explicit frozen-oracle comparison of menu decoder policies.
#![allow(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    reason = "Image policy fixtures retain diagnostic floating-point ratios and complete decoder matrices"
)]
use dvda_native::{images::Images, media::Callbacks};
use std::{fs, io::Cursor, path::PathBuf};

#[derive(Default)]
struct Events(String);
impl Callbacks for Events {
    fn emit(&mut self, _: i32, text: &str) {
        self.0.push_str(text);
    }
    fn cancelled(&mut self) -> bool {
        false
    }
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires an explicit frozen Windows x64 image oracle and external CMYK ICC profile"]
fn decoder_color_and_first_frame_policies() {
    let oracle = PathBuf::from(
        std::env::var_os("DVDA_IMAGE_NATIVE_DIR").expect("Set DVDA_IMAGE_NATIVE_DIR"),
    )
    .join("dvda-image.dll");
    let images = Images::load(&oracle).unwrap();
    let root = Temporary(std::env::temp_dir().join(format!(
        "dvda-image-policies-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    )));
    fs::create_dir(&root.0).unwrap();
    let profile = std::env::var("DVDA_IMAGE_TEST_CMYK_ICC")
        .expect("Set DVDA_IMAGE_TEST_CMYK_ICC to an external CMYK ICC profile");
    assert!(std::path::Path::new(&profile).is_file());
    let cases: &[(&str, &[&str], u8)] = &[
        (
            "icc-cmyk.jpg",
            &[
                "-size",
                "17x11",
                "xc:#204060",
                "-colorspace",
                "CMYK",
                "-quality",
                "100",
            ],
            3,
        ),
        (
            "rgb.png",
            &["-size", "17x11", "xc:#204060", "-depth", "8"],
            0,
        ),
        (
            "gamma.png",
            &[
                "-size",
                "17x11",
                "xc:#204060",
                "-set",
                "gamma",
                "0.7",
                "-depth",
                "8",
            ],
            0,
        ),
        (
            "sixteen.png",
            &[
                "-size",
                "17x11",
                "gradient:#123456-#abcdef",
                "-depth",
                "16",
                "-define",
                "png:bit-depth=16",
            ],
            1,
        ),
        (
            "cmyk.jpg",
            &[
                "-size",
                "17x11",
                "xc:#204060",
                "-colorspace",
                "CMYK",
                "-quality",
                "100",
            ],
            3,
        ),
        (
            "orientation.jpg",
            &[
                "-size",
                "17x11",
                "gradient:red-blue",
                "-orient",
                "RightTop",
                "-quality",
                "100",
            ],
            3,
        ),
        (
            "animated.webp",
            &[
                "-size",
                "17x11",
                "-delay",
                "10",
                "xc:red",
                "xc:blue",
                "-loop",
                "0",
                "-define",
                "webp:lossless=true",
            ],
            0,
        ),
    ];
    let mut regressions = Vec::new();
    for &(name, options, tolerance) in cases {
        let path = root.0.join(name);
        let mut arguments = vec!["convert".to_owned()];
        arguments.extend(options.iter().map(|s| (*s).to_owned()));
        arguments.push(path.to_string_lossy().into_owned());
        let mut events = Events::default();
        assert_eq!(
            images.run(&arguments, &mut events).unwrap(),
            0,
            "{name}: {}",
            events.0
        );
        if name == "icc-cmyk.jpg" {
            let jpeg = fs::read(&path).unwrap();
            assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
            let icc = fs::read(&profile).unwrap();
            assert!(icc.len() >= 128 && &icc[36..40] == b"acsp");
            assert_eq!(&icc[16..20], b"CMYK");
            let chunks: Vec<_> = icc.chunks(65_519).collect();
            let count = u8::try_from(chunks.len()).unwrap();
            let mut tagged = jpeg[..2].to_vec();
            // JPEG ICC profiles are split across numbered APP2 segments.
            for (index, chunk) in chunks.iter().enumerate() {
                tagged.extend_from_slice(&[0xff, 0xe2]);
                tagged.extend_from_slice(&u16::try_from(chunk.len() + 16).unwrap().to_be_bytes());
                tagged.extend_from_slice(b"ICC_PROFILE\0");
                tagged.extend_from_slice(&[u8::try_from(index + 1).unwrap(), count]);
                tagged.extend_from_slice(chunk);
            }
            tagged.extend_from_slice(&jpeg[2..]);
            fs::write(&path, tagged).unwrap();
        }
        let mut properties = Events::default();
        assert_eq!(
            images
                .run(
                    &[
                        "identify".to_owned(),
                        "-format".to_owned(),
                        "%m|%z|%[gamma]|%n\n".to_owned(),
                        path.to_string_lossy().into_owned(),
                    ],
                    &mut properties
                )
                .unwrap(),
            0
        );
        eprintln!("{name}: {}", properties.0.trim());
        if name == "icc-cmyk.jpg" {
            assert!(
                fs::read(&path)
                    .unwrap()
                    .windows(12)
                    .any(|bytes| bytes == b"ICC_PROFILE\0"),
                "ICC fixture has no embedded profile"
            );
        }
        if name == "gamma.png" {
            assert!(properties.0.contains("|0.7|"));
        }
        if name == "sixteen.png" {
            assert!(properties.0.contains("PNG|16|"));
        }
        if name == "animated.webp" {
            assert!(
                properties.0.lines().count() >= 2,
                "animation fixture has only one frame"
            );
        }
        if name == "orientation.jpg" {
            let jpeg = fs::read(&path).unwrap();
            assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
            // APP1 containing a little-endian TIFF IFD with Orientation=6.
            let exif = [
                0xff, 0xe1, 0, 34, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1,
                0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
            ];
            let mut tagged = jpeg[..2].to_vec();
            tagged.extend_from_slice(&exif);
            tagged.extend_from_slice(&jpeg[2..]);
            fs::write(&path, tagged).unwrap();
            let mut orientation = Events::default();
            assert_eq!(
                images
                    .run(
                        &[
                            "identify".to_owned(),
                            "-format".to_owned(),
                            "%[orientation]".to_owned(),
                            path.to_string_lossy().into_owned(),
                        ],
                        &mut orientation
                    )
                    .unwrap(),
                0
            );
            assert_eq!(orientation.0.trim(), "RightTop");
        }
        // Compare explicitly converted RGB output; this isolates decoder behavior.
        // Raw RGBA export of a CMYK wand exposes CMY channels instead.
        let rendered = root.0.join(format!("{name}.png"));
        let arguments = vec![
            "convert".to_owned(),
            format!("{}[0]", path.display()),
            "-colorspace".to_owned(),
            "sRGB".to_owned(),
            format!("PNG32:{}", rendered.display()),
        ];
        assert_eq!(
            images.run(&arguments, &mut events).unwrap(),
            0,
            "{name}: {}",
            events.0
        );
        let (width, height, expected) = images.read_rgba(&rendered).unwrap();
        let actual = dvda_image::codec::decode(Cursor::new(fs::read(&path).unwrap())).unwrap();
        assert_eq!(
            (actual.dimensions().width(), actual.dimensions().height()),
            (width, height),
            "{name}"
        );
        let max_delta = actual
            .rgba()
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        eprintln!("{name}: {width}x{height}, maximum channel delta={max_delta}");
        if matches!(name, "cmyk.jpg" | "icc-cmyk.jpg" | "animated.webp") {
            use dvda_image::geometry::{Dimensions, ResizeMode};
            for dim in [0, 35] {
                let background = root.0.join(format!("{name}-{dim}-background.jpg"));
                let mut args = vec![
                    "convert".to_owned(),
                    path.to_string_lossy().into_owned(),
                    "-resize".to_owned(),
                    "720x576^".to_owned(),
                    "-gravity".to_owned(),
                    "center".to_owned(),
                    "-extent".to_owned(),
                    "720x576".to_owned(),
                ];
                if dim > 0 {
                    args.extend(["-brightness-contrast".to_owned(), format!("-{dim}x0")]);
                }
                args.extend([
                    "-quality".to_owned(),
                    "88".to_owned(),
                    background.to_string_lossy().into_owned(),
                ]);
                assert_eq!(images.run(&args, &mut events).unwrap(), 0, "{}", events.0);
                // Multi-frame input may produce numbered outputs in the historical path.
                let output = if background.exists() {
                    background.clone()
                } else {
                    background.with_file_name(format!("{name}-{dim}-background-0.jpg"))
                };
                assert!(output.exists(), "historical menu produced no first image");
                eprintln!(
                    "{name}: historical background uses requested filename={}",
                    background.exists()
                );
                if name == "animated.webp" {
                    assert!(
                        !background.exists(),
                        "fixture must expose historical numbered outputs"
                    );
                }
                let expected =
                    dvda_image::codec::decode(Cursor::new(fs::read(&output).unwrap())).unwrap();
                let frame = Dimensions::new(720, 576).unwrap();
                let mut current = actual
                    .resized(frame, ResizeMode::Fill)
                    .unwrap()
                    .centered_extent(frame, [0, 0, 0, 255])
                    .unwrap();
                current.dim(dim);
                let mut jpeg = Vec::new();
                dvda_image::codec::write_jpeg(&current, &mut jpeg, 88, [0; 3]).unwrap();
                let current = dvda_image::codec::decode(Cursor::new(jpeg)).unwrap();
                let sum: u64 = current
                    .rgba()
                    .iter()
                    .zip(expected.rgba())
                    .map(|(a, b)| u64::from(a.abs_diff(*b)))
                    .sum();
                let mean = sum as f64 / current.rgba().len() as f64;
                eprintln!(
                    "{name}: historical background dim={dim}, mean channel delta={mean:.4}, old pixel={:?}, Rust pixel={:?}",
                    &expected.rgba()[..4],
                    &current.rgba()[..4]
                );
                if matches!(name, "cmyk.jpg" | "icc-cmyk.jpg") && dim > 0 {
                    // Approved defect fix: dim RGB, not subtractive CMYK channels.
                    let corrected = root.0.join(format!("{name}-{dim}-rgb-background.jpg"));
                    let mut corrected_args = args.clone();
                    corrected_args.splice(2..2, ["-colorspace".to_owned(), "sRGB".to_owned()]);
                    *corrected_args.last_mut().unwrap() = corrected.to_string_lossy().into_owned();
                    assert_eq!(
                        images.run(&corrected_args, &mut events).unwrap(),
                        0,
                        "{}",
                        events.0
                    );
                    let corrected =
                        dvda_image::codec::decode(Cursor::new(fs::read(corrected).unwrap()))
                            .unwrap();
                    let corrected_mean = current
                        .rgba()
                        .iter()
                        .zip(corrected.rgba())
                        .map(|(a, b)| u64::from(a.abs_diff(*b)))
                        .sum::<u64>() as f64
                        / current.rgba().len() as f64;
                    assert!(
                        corrected_mean <= 3.0,
                        "{name}: RGB dim delta={corrected_mean}"
                    );
                    assert!(mean > 3.0, "CMYK fixture must expose the historical defect");
                    for (before, after) in actual.rgba()[..3].iter().zip(&current.rgba()[..3]) {
                        assert!(after <= before, "CMYK dim must not brighten the image");
                    }
                } else if mean > 3.0 {
                    regressions.push(format!(
                        "{name}: historical background dim={dim} mean delta={mean}"
                    ));
                }
            }
        }
        if matches!(name, "cmyk.jpg" | "icc-cmyk.jpg" | "animated.webp") {
            use dvda_image::geometry::{Dimensions, ResizeMode};
            let still = root.0.join(format!("{name}-still.jpg"));
            let args = [
                "convert",
                path.to_str().unwrap(),
                "-resize",
                "93.75%x100%!",
                "-resize",
                "720x576",
                "-background",
                "black",
                "-gravity",
                "center",
                "-extent",
                "720x576",
                "-quality",
                "92",
                still.to_str().unwrap(),
            ]
            .map(str::to_owned);
            assert_eq!(images.run(&args, &mut events).unwrap(), 0, "{}", events.0);
            let old = if still.exists() {
                still
            } else {
                root.0.join(format!("{name}-still-0.jpg"))
            };
            let expected = dvda_image::codec::decode(Cursor::new(fs::read(old).unwrap())).unwrap();
            let size = actual.dimensions();
            let compressed =
                Dimensions::new(((size.width() * 15 + 8) / 16).max(1), size.height()).unwrap();
            let frame = Dimensions::new(720, 576).unwrap();
            let current = actual
                .resized(compressed, ResizeMode::Exact)
                .unwrap()
                .resized(frame, ResizeMode::Fit)
                .unwrap()
                .centered_extent(frame, [0, 0, 0, 255])
                .unwrap();
            let mut jpeg = Vec::new();
            dvda_image::codec::write_jpeg(&current, &mut jpeg, 92, [0; 3]).unwrap();
            let current = dvda_image::codec::decode(Cursor::new(jpeg)).unwrap();
            assert_eq!(current.dimensions(), expected.dimensions());
            let mean = current
                .rgba()
                .iter()
                .zip(expected.rgba())
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum::<u64>() as f64
                / current.rgba().len() as f64;
            eprintln!("{name}: historical still mean channel delta={mean:.4}");
            if mean > 3.0 {
                regressions.push(format!("{name}: historical still mean delta={mean}"));
            }
        }
        assert!(
            max_delta <= tolerance,
            "{name}: maximum channel delta {max_delta} exceeds {tolerance}"
        );
    }
    assert!(regressions.is_empty(), "{}", regressions.join("\n"));
}
