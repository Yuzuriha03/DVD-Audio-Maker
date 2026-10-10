//! Exercise the public entry and independently read its ISO directory records.
use dvda_author::{Callbacks, Silent, command, menu};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct NoMenu;
impl menu::Backend for NoMenu {
    fn image_run(&mut self, _: Vec<String>, _: &mut dyn Callbacks) -> Result<(), String> {
        panic!("Unexpected menu operation")
    }
    fn write_y4m(
        &mut self,
        _: &Path,
        _: &Path,
        _: &str,
        _: &str,
        _: &mut dyn Callbacks,
    ) -> Result<(), String> {
        panic!("Unexpected menu operation")
    }
    fn create_mpg(
        &mut self,
        _: &Path,
        _: &Path,
        _: &Path,
        _: &str,
        _: &str,
        _: bool,
        _: &mut dyn Callbacks,
    ) -> Result<(), String> {
        panic!("Unexpected menu operation")
    }
    fn subpictures(
        &mut self,
        _: &Path,
        _: &Path,
        _: &Path,
        _: &mut dyn Callbacks,
    ) -> Result<(), String> {
        panic!("Unexpected menu operation")
    }
    fn navigation(&mut self, _: &Path, _: &Path, _: &mut dyn Callbacks) -> Result<(), String> {
        panic!("Unexpected menu operation")
    }
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "dvda-full-author-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn wav(path: &Path, channels: u16, samples: u32) {
    let size = samples * channels as u32 * 2;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + size).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(48000u32.to_le_bytes());
    bytes.extend((48000 * channels as u32 * 2).to_le_bytes());
    bytes.extend((channels * 2).to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(size.to_le_bytes());
    for index in 0..samples * channels as u32 {
        bytes.extend((index as i16).wrapping_mul(37).to_le_bytes());
    }
    fs::write(path, bytes).unwrap();
}
fn args(root: &Path) -> Vec<String> {
    [
        "-g".into(),
        root.join("a.wav").display().to_string(),
        root.join("b.wav").display().to_string(),
        "-z".into(),
        root.join("a.wav").display().to_string(),
        "-g".into(),
        root.join("b.wav").display().to_string(),
        "-o".into(),
        root.join("disc").display().to_string(),
        "-D".into(),
        root.join("temp").display().to_string(),
        format!("--iso={}", root.join("disc.iso").display()),
        "--iso-volume=RUST_AUTHOR".into(),
    ]
    .into()
}
fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}
fn records(iso: &[u8], lba: u32, size: u32) -> Vec<(String, u32, u32)> {
    let base = lba as usize * 2048;
    let mut at = base;
    let mut result = Vec::new();
    while at < base + size as usize {
        let length = iso[at] as usize;
        if length == 0 {
            at = (at / 2048 + 1) * 2048;
            continue;
        }
        assert!(at % 2048 + length <= 2048);
        let name = &iso[at + 33..at + 33 + iso[at + 32] as usize];
        if !matches!(name, [0] | [1]) {
            result.push((
                String::from_utf8(name.to_vec()).unwrap(),
                le32(iso, at + 2),
                le32(iso, at + 10),
            ));
        }
        at += length;
    }
    result
}
#[test]
fn cli_assembles_multiple_groups_and_samg_addresses_match_real_iso() {
    let directory = Directory::new();
    wav(&directory.0.join("a.wav"), 2, 4800);
    wav(&directory.0.join("b.wav"), 2, 4800);
    command::run(&args(&directory.0), &mut NoMenu, &mut Silent).unwrap();
    let image = fs::read(directory.0.join("disc.iso")).unwrap();
    assert_eq!(&image[16 * 2048 + 1..16 * 2048 + 6], b"CD001");
    let root = 16 * 2048 + 156;
    let audio = records(&image, le32(&image, root + 2), le32(&image, root + 10))
        .into_iter()
        .find(|(n, _, _)| n == "AUDIO_TS")
        .unwrap();
    let files = records(&image, audio.1, audio.2);
    let simple = fs::read(directory.0.join("disc/AUDIO_TS/AUDIO_PP.IFO")).unwrap();
    assert_eq!(&simple[..12], b"DVDAUDIOSAPP");
    assert_eq!(u16::from_be_bytes(simple[12..14].try_into().unwrap()), 4);
    for (group, start, count) in [(1, 0, 3), (2, 3, 1)] {
        let file = files
            .iter()
            .find(|(n, _, _)| n == &format!("ATS_{group:02}_1.AOB;1"))
            .unwrap();
        for track in start..start + count {
            let row = 16 + 52 * track;
            let first = be32(&simple, row + 40);
            let last = be32(&simple, row + 48);
            assert!(first >= file.1 && last < file.1 + file.2.div_ceil(2048));
            assert_eq!(
                &image[first as usize * 2048..first as usize * 2048 + 4],
                b"\0\0\x01\xba"
            );
        }
    }
    for (name, lba, size) in files {
        let source = fs::read(
            directory
                .0
                .join("disc/AUDIO_TS")
                .join(name.trim_end_matches(";1")),
        )
        .unwrap();
        assert_eq!(source.len(), size as usize);
        assert_eq!(
            &image[lba as usize * 2048..lba as usize * 2048 + size as usize],
            source
        );
    }
    for group in [1, 2] {
        assert_eq!(
            fs::read(
                directory
                    .0
                    .join(format!("disc/AUDIO_TS/ATS_{group:02}_0.IFO"))
            )
            .unwrap(),
            fs::read(
                directory
                    .0
                    .join(format!("disc/AUDIO_TS/ATS_{group:02}_0.BUP"))
            )
            .unwrap()
        );
    }
}
#[test]
fn failure_cleans_staging_preserves_existing_iso_and_output() {
    let directory = Directory::new();
    wav(&directory.0.join("a.wav"), 2, 4800);
    fs::write(directory.0.join("b.wav"), b"invalid input").unwrap();
    fs::write(directory.0.join("disc.iso"), b"previous ISO").unwrap();
    assert!(command::run(&args(&directory.0), &mut NoMenu, &mut Silent).is_err());
    assert!(!directory.0.join("disc").exists());
    assert_eq!(
        fs::read(directory.0.join("disc.iso")).unwrap(),
        b"previous ISO"
    );
    assert!(fs::read_dir(&directory.0).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".dvda-author-")
    }));
    fs::create_dir(directory.0.join("disc")).unwrap();
    fs::write(directory.0.join("disc/keep"), b"keep").unwrap();
    assert!(command::run(&args(&directory.0), &mut NoMenu, &mut Silent).is_err());
    assert_eq!(fs::read(directory.0.join("disc/keep")).unwrap(), b"keep");
}
#[test]
fn checked_atsi_uses_title_formats_and_codec_channel_assignment_identity() {
    use dvda_author::{atsi, samg};
    let track = samg::Track {
        mlp: true,
        channels: 2,
        bits: 16,
        rate: 48000,
        channel_assignment: 1,
        first_pts: 98,
        pts_length: 9000,
        first_sector: 0,
        last_sector: 1,
    };
    let surround = samg::Track {
        mlp: false,
        channels: 6,
        bits: 24,
        rate: 48000,
        channel_assignment: 12,
        ..track
    };
    let changed = samg::Track {
        channel_assignment: 13,
        ..surround
    };
    let titles = [
        atsi::Title {
            tracks: vec![track.into(), track.into(), track.into()],
        },
        atsi::Title {
            tracks: vec![surround.into()],
        },
        atsi::Title {
            tracks: vec![changed.into()],
        },
    ];
    let bytes = atsi::encode_checked(&titles, &atsi::Options::default())
        .unwrap()
        .bytes;
    assert_eq!(&bytes[0x110..0x115], &[0, 0, 0x22, 0, 12]);
    assert_eq!(&bytes[0x120..0x125], &[0, 0, 0x22, 0, 13]);
    assert_eq!(&bytes[0x811..0x813], &[1, 0]);
    let mixed = atsi::Title {
        tracks: vec![
            track.into(),
            samg::Track {
                mlp: false,
                ..track
            }
            .into(),
        ],
    };
    assert!(atsi::encode_checked(&[mixed], &atsi::Options::default()).is_err());
}
