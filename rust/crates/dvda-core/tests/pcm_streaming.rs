//! Independent integer fixtures: no C# runner or codec DLL is needed.
use dvda_core::pcm::{Job, normalize};
use dvda_native::media::Callbacks;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "dvda-pcm-中文-日本語-🎵-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[derive(Default)]
struct Events {
    polls: usize,
    cancel_at: Option<usize>,
    panic_at: Option<usize>,
}
impl Callbacks for Events {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        self.polls += 1;
        assert!(
            self.panic_at != Some(self.polls),
            "Intentional PCM callback panic"
        );
        self.cancel_at == Some(self.polls)
    }
}
fn job(input: &Path, output: &Path, rate: i32, bits: i32) -> Job {
    Job {
        source: input.to_str().unwrap().into(),
        destination: output.into(),
        rate,
        bits,
    }
}
fn fixture(
    rate: u32,
    precision: u32,
    stored_bits: u32,
    channels: u16,
    frames: usize,
    mask: u32,
    wide: bool,
) -> Vec<u8> {
    let width = if wide {
        4
    } else if stored_bits == 16 {
        2
    } else {
        3
    };
    let mut data = Vec::new();
    for n in 0..frames {
        for channel in 0..usize::from(channels) {
            let value = (((n * (97 + channel * 18) + channel * 53) % 100001) as i32 - 50000)
                & !((1 << (24 - precision)) - 1);
            let stored = if wide {
                value << 8
            } else if width == 2 {
                value >> 8
            } else {
                value
            };
            data.extend_from_slice(&stored.to_le_bytes()[..width]);
        }
    }
    let size = data.len() as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(60 + size + size % 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&0xfffeu16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * width as u32).to_le_bytes());
    bytes.extend_from_slice(&(channels * width as u16).to_le_bytes());
    bytes.extend_from_slice(&(width as u16 * 8).to_le_bytes());
    bytes.extend_from_slice(&22u16.to_le_bytes());
    bytes.extend_from_slice(&(stored_bits as u16).to_le_bytes());
    bytes.extend_from_slice(&mask.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend(data);
    if size & 1 != 0 {
        bytes.push(0);
    }
    bytes
}

#[test]
fn complete_wav_matrix_including_wide_storage() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.wav");
    let mut count = 0;
    for rate in [44100, 48000, 88200, 96000, 176400, 192000] {
        for precision in [16, 20, 24] {
            for channels in 1..=if rate > 96000 { 2 } else { 6 } {
                let mask = [0, 4, 3, 7, 0x33, 0x37, 0x3f][channels as usize];
                for wide in [false, true] {
                    fs::write(
                        &input,
                        fixture(rate, precision, precision, channels, 8193, mask, wide),
                    )
                    .unwrap();
                    for target in [16, 20, 24] {
                        let result = normalize(
                            job(&input, &output, rate as i32, target),
                            &mut Events::default(),
                        );
                        if target < precision as i32 {
                            assert_eq!(result.unwrap_err().kind, "InvalidData");
                            assert!(!output.exists());
                        } else {
                            let expected = fixture(
                                rate,
                                precision,
                                target as u32,
                                channels,
                                8193,
                                mask,
                                false,
                            );
                            assert_eq!(result.unwrap(), expected.len() as u64);
                            assert_eq!(
                                fs::read(&output).unwrap(),
                                expected,
                                "{rate}/{precision}/{channels} -> {target}, wide={wide}"
                            );
                            fs::remove_file(&output).unwrap();
                        }
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 504);
}

#[test]
fn layout_failures_cancellation_and_owned_cleanup() {
    let root = Scratch::new();
    let input = root.0.join("input.wav");
    let output = root.0.join("output.wav");
    let source = fixture(48000, 24, 24, 6, 16385, 0x60f, false);
    fs::write(&input, &source).unwrap();
    normalize(job(&input, &output, 48000, 24), &mut Events::default()).unwrap();
    assert_eq!(
        fs::read(&output).unwrap(),
        fixture(48000, 24, 24, 6, 16385, 0x3f, false)
    );
    fs::remove_file(&output).unwrap();
    for poll in 1..=5 {
        let result = normalize(
            job(&input, &output, 48000, 24),
            &mut Events {
                cancel_at: Some(poll),
                ..Events::default()
            },
        );
        assert_eq!(result.unwrap_err().kind, "Cancelled");
        assert!(!output.exists());
        assert_eq!(fs::read(&input).unwrap(), source);
    }
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        normalize(
            job(&input, &output, 48000, 24),
            &mut Events {
                panic_at: Some(3),
                ..Events::default()
            },
        )
    }));
    assert!(panic.is_err());
    assert!(!output.exists());
    fs::write(&output, b"preserve").unwrap();
    assert_eq!(
        normalize(job(&input, &output, 48000, 24), &mut Events::default())
            .unwrap_err()
            .kind,
        "Io"
    );
    assert_eq!(fs::read(&output).unwrap(), b"preserve");
    fs::remove_file(&output).unwrap();
    for mask in [0x22fu32, 0x30f] {
        let bytes = fixture(48000, 24, 24, 6, 817, mask, false);
        fs::write(&input, bytes).unwrap();
        assert_eq!(
            normalize(job(&input, &output, 48000, 24), &mut Events::default())
                .unwrap_err()
                .kind,
            "InvalidData"
        );
        assert!(!output.exists());
    }
    let mut bytes = fixture(48000, 24, 24, 2, 817, 3, true);
    bytes[68] = 1;
    fs::write(&input, bytes).unwrap();
    assert_eq!(
        normalize(job(&input, &output, 48000, 24), &mut Events::default())
            .unwrap_err()
            .kind,
        "InvalidData"
    );
    assert!(!output.exists());
    // Only the odd terminal RIFF alignment byte may be omitted.
    let mut bytes = fixture(48000, 24, 24, 1, 817, 4, false);
    bytes.pop();
    let size = bytes.len() as u32 - 8;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    fs::write(&input, bytes).unwrap();
    normalize(job(&input, &output, 48000, 24), &mut Events::default()).unwrap();
    assert_eq!(
        fs::read(&output).unwrap(),
        fixture(48000, 24, 24, 1, 817, 4, false)
    );
}
