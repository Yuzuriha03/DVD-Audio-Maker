use dvda_native::disc_verify::NativeDiscVerifier;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "authored-lpcm-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn wav(path: &Path, rate: u32, bits: u16, channels: u16, frames: u32, seed: u32) {
    let width = (bits / 8) as usize;
    let mask = [4u32, 3, 7, 0x33, 0x37, 0x3f][channels as usize - 1];
    let mut data = Vec::with_capacity(frames as usize * channels as usize * width);
    for i in 0..frames {
        for channel in 0..channels as u32 {
            let sample = (i as u64 * 7919 + channel as u64 * 32771 + seed as u64 * 104729)
                & ((1u64 << bits) - 1);
            data.extend_from_slice(&sample.to_le_bytes()[..width]);
        }
    }
    let mut body = Vec::new();
    body.extend_from_slice(b"WAVEfmt ");
    body.extend_from_slice(&40u32.to_le_bytes());
    body.extend_from_slice(&0xfffeu16.to_le_bytes());
    body.extend_from_slice(&channels.to_le_bytes());
    body.extend_from_slice(&rate.to_le_bytes());
    body.extend_from_slice(&(rate * channels as u32 * width as u32).to_le_bytes());
    body.extend_from_slice(&(channels * width as u16).to_le_bytes());
    body.extend_from_slice(&bits.to_le_bytes());
    body.extend_from_slice(&22u16.to_le_bytes());
    body.extend_from_slice(&bits.to_le_bytes());
    body.extend_from_slice(&mask.to_le_bytes());
    body.extend_from_slice(&[1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(&data);
    if data.len() % 2 != 0 {
        body.push(0);
    }
    let mut file = File::create(path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(body.len() as u32).to_le_bytes()).unwrap();
    file.write_all(&body).unwrap();
}

fn authored(author: &Path, work: &Path, sources: &[PathBuf], separate: bool) -> Vec<u8> {
    let output = work.join("disc");
    let temp = work.join("tmp");
    fs::create_dir(&temp).unwrap();
    let mut cmd = Command::new(author);
    cmd.arg("-g").arg(&sources[0]);
    if separate {
        cmd.arg("-z");
    }
    let log = cmd
        .arg(&sources[1])
        .arg("-o")
        .arg(&output)
        .arg("-D")
        .arg(&temp)
        .args(["-W", "-P0", "-n"])
        .output()
        .expect("launch DVD-Audio author");
    assert!(
        log.status.success(),
        "author failed for {}: {}",
        work.display(),
        String::from_utf8_lossy(&log.stdout)
    );
    let mut segments: Vec<_> = fs::read_dir(output.join("AUDIO_TS"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ATS_01_")
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("AOB"))
        })
        .collect();
    segments.sort();
    assert!(!segments.is_empty(), "no authored AOB segments");
    let mut payload = Vec::new();
    for segment in segments {
        File::open(segment)
            .unwrap()
            .read_to_end(&mut payload)
            .unwrap();
    }
    assert_eq!(payload.len() % 2048, 0);
    payload
}

struct Scenario {
    rate: u32,
    bits: u16,
    channels: u16,
    frames: u32,
    separate: bool,
    tiny: bool,
}

fn case(author: &Path, root: &Path, scenario: Scenario) {
    let Scenario {
        rate,
        bits,
        channels,
        frames,
        separate,
        tiny,
    } = scenario;
    let label = format!(
        "{rate}-{bits}-{channels}-{frames}-{}",
        if separate { "titles" } else { "gapless" }
    );
    let work = root.join(&label);
    fs::create_dir(&work).unwrap();
    let sources: Vec<_> = (0..2)
        .map(|index| {
            let source = work.join(format!("{index}.wav"));
            wav(
                &source,
                rate,
                bits,
                channels,
                frames + if tiny { 0 } else { index * 2 },
                index + 1,
            );
            source
        })
        .collect();
    let payload = authored(author, &work, &sources, separate);
    let ends = [u8::from(separate), 1];
    let verifier = NativeDiscVerifier::load().unwrap();
    let verify = |data: Vec<u8>| {
        let chunks = data.chunks(128 * 1024).map(|chunk| Ok(chunk.to_vec()));
        verifier.verify_lpcm(&sources, &ends, chunks)
    };
    let result = verify(payload.clone()).unwrap_or_else(|e| panic!("{label}: {e}"));
    assert!(result.bytes > 0, "{label}: no audio compared");
    if tiny {
        let expected_frames = if separate {
            2 * (frames + frames % 2)
        } else {
            2 * frames
        };
        assert_eq!(
            result.bytes,
            expected_frames as u64 * channels as u64 * (bits / 8) as u64,
            "{label}: wrong PCM length"
        );
    }
    let private = (14..200)
        .find(|&i| payload[i..i + 4] == [0, 0, 1, 0xbd])
        .map(|q| q + 9 + payload[q + 8] as usize)
        .expect("private stream packet");
    let audio = private + 4 + payload[private + 3] as usize;
    assert!(audio < payload.len(), "{label}: no private audio byte");
    let mut changed = payload.clone();
    changed[audio] ^= 1;
    assert!(
        verify(changed).is_err(),
        "{label}: corrupted audio accepted"
    );
    let mut changed = payload.clone();
    changed[private + 7] ^= 0x20;
    assert!(
        verify(changed).is_err(),
        "{label}: wrong bit depth accepted"
    );
    assert!(
        verify(payload[..payload.len() - 2048].to_vec()).is_err(),
        "{label}: truncated payload accepted"
    );
}

#[test]
fn authored_lpcm_formats_and_short_titles() {
    let Some(author) = std::env::var_os("DVDA_TEST_AUTHOR").map(PathBuf::from) else {
        eprintln!("Set DVDA_TEST_AUTHOR to run the authored LPCM matrix");
        return;
    };
    assert!(
        author.is_file(),
        "DVDA_TEST_AUTHOR is not a file: {}",
        author.display()
    );
    let workspace = Workspace::new();
    let quick = std::env::var_os("DVDA_TEST_AUTHOR_QUICK").is_some();
    let formats: Vec<(u32, u16, u16)> = if quick {
        vec![(48000, 16, 2), (48000, 24, 6), (192000, 24, 2)]
    } else {
        [44100, 48000, 88200, 96000, 176400, 192000]
            .into_iter()
            .flat_map(|rate| {
                [16, 24].into_iter().flat_map(move |bits| {
                    (1..=6)
                        .filter(move |&channels| {
                            rate * bits as u32 * channels as u32 <= 9_600_000
                                && (rate <= 96000 || channels <= 2)
                        })
                        .map(move |channels| (rate, bits, channels))
                })
            })
            .collect()
    };
    for (rate, bits, channels) in formats {
        for separate in [false, true] {
            case(
                &author,
                &workspace.0,
                Scenario {
                    rate,
                    bits,
                    channels,
                    frames: rate / 8 + 1,
                    separate,
                    tiny: false,
                },
            );
        }
    }
    for (bits, channels) in [(16, 1), (24, 1), (24, 2), (24, 6)] {
        for frames in [1, 2, 3, 127, 501] {
            for separate in [false, true] {
                case(
                    &author,
                    &workspace.0,
                    Scenario {
                        rate: 48000,
                        bits,
                        channels,
                        frames,
                        separate,
                        tiny: true,
                    },
                );
            }
        }
    }
}
