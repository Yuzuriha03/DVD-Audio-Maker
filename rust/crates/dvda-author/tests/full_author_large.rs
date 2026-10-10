//! Capacity, cancellation and transaction tests through the production command.
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
        let path = std::env::temp_dir().join(format!(
            "dvda-full-large-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let mut wave = Vec::new();
        wave.extend(b"RIFF");
        wave.extend(44u32.to_le_bytes());
        wave.extend(b"WAVEfmt ");
        wave.extend(16u32.to_le_bytes());
        wave.extend(1u16.to_le_bytes());
        wave.extend(2u16.to_le_bytes());
        wave.extend(48000u32.to_le_bytes());
        wave.extend(192000u32.to_le_bytes());
        wave.extend(4u16.to_le_bytes());
        wave.extend(16u16.to_le_bytes());
        wave.extend(b"data");
        wave.extend(8u32.to_le_bytes());
        wave.extend([1, 0, 2, 0, 3, 0, 4, 0]);
        fs::write(path.join("音楽 한글.wav"), wave).unwrap();
        Self(path)
    }
    fn arguments(&self, groups: usize, tracks: usize) -> Vec<String> {
        let mut result = Vec::new();
        for _ in 0..groups {
            result.push("-g".into());
            for track in 0..tracks {
                if track > 0 {
                    result.push("-z".into());
                }
                result.push(self.0.join("音楽 한글.wav").display().to_string());
            }
        }
        result.extend([
            "-o".into(),
            self.0.join("disc").display().to_string(),
            "-D".into(),
            self.0.join("temp").display().to_string(),
            format!("--iso={}", self.0.join("disc.iso").display()),
        ]);
        result
    }
    fn no_staging(&self) {
        assert!(fs::read_dir(&self.0).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.starts_with(".dvda-author-") && !name.starts_with(".dvda-iso-")
        }));
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn be16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn be32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn ninety_nine_and_eight_hundred_ninety_one_titles_expand_all_tables() {
    for groups in [1, 9] {
        let work = Directory::new();
        command::run(&work.arguments(groups, 99), &mut NoMenu, &mut Silent).unwrap();
        let directory = work.0.join("disc/AUDIO_TS");
        let simple = fs::read(directory.join("AUDIO_PP.IFO")).unwrap();
        let count = groups * 99;
        let sectors = ((16 + 52 * count).div_ceil(2048) * 8).max(64);
        assert_eq!(simple.len(), sectors * 2048);
        assert_eq!(usize::from(be16(&simple, 12)), count);
        let matrix = simple.len() / 8;
        assert!(simple.as_chunks::<8>().1.is_empty());
        for copy in 1..8 {
            assert_eq!(
                &simple[..matrix],
                &simple[copy * matrix..(copy + 1) * matrix]
            );
        }
        let amg = fs::read(directory.join("AUDIO_TS.IFO")).unwrap();
        let first = be32(&amg, 0xc4) as usize * 2048;
        let second = be32(&amg, 0xc8) as usize * 2048;
        assert_eq!(usize::from(be16(&amg, first)), count);
        assert_eq!(usize::from(be16(&amg, second)), count);
        assert!(first + 4 + 14 * count <= second);
        assert!(second + 4 + 14 * count <= amg.len());
        let image = fs::read(work.0.join("disc.iso")).unwrap();
        for group in 1..=groups {
            let atsi = fs::read(directory.join(format!("ATS_{group:02}_0.IFO"))).unwrap();
            assert_eq!(be16(&atsi, 0x800), 99);
            assert_eq!(
                atsi,
                fs::read(directory.join(format!("ATS_{group:02}_0.BUP"))).unwrap()
            );
            for track in 0..99 {
                let title = 0x800 + be32(&atsi, 0x80c + track * 8) as usize;
                assert_eq!(atsi[title + 2], 1);
                let row = 16 + 52 * ((group - 1) * 99 + track);
                assert_eq!(&simple[row + 2..row + 4], &[group as u8, track as u8 + 1]);
                let sector = be32(&simple, row + 40) as usize;
                assert_eq!(be32(&simple, row + 44), sector as u32);
                assert_eq!(be32(&simple, row + 48), sector as u32);
                assert_eq!(&image[sector * 2048..sector * 2048 + 4], b"\0\0\x01\xba");
            }
        }
        work.no_staging();
    }
}

struct Cancel {
    trigger: &'static str,
    iso: bool,
    cancel: bool,
    after_progress: bool,
}
impl Callbacks for Cancel {
    fn emit(&mut self, _: i32, text: &str) {
        if text.contains(self.trigger) {
            self.iso = true;
            self.cancel = !self.after_progress;
        }
    }
    fn cancelled(&mut self) -> bool {
        self.cancel
    }
    fn progress(&mut self, completed: u64, _: u64) {
        if self.iso && self.after_progress && completed >= 32768 {
            self.cancel = true;
        }
    }
}

#[test]
fn cancellation_before_and_during_iso_preserves_previous_image_and_disc() {
    for (trigger, after_progress) in [
        ("Creating ATS_01 audio", false),
        ("Creating AUDIO_TS.IFO", false),
        ("Writing ISO image", false),
        ("Writing ISO image", true),
    ] {
        let work = Directory::new();
        fs::write(work.0.join("disc.iso"), b"previous image").unwrap();
        let mut events = Cancel {
            trigger,
            iso: false,
            cancel: false,
            after_progress,
        };
        let error = command::run(&work.arguments(1, 3), &mut NoMenu, &mut events).unwrap_err();
        assert!(error.contains("cancelled"), "{trigger}: {error}");
        assert!(
            !work.0.join("disc").exists(),
            "Disc committed after cancellation at {trigger}"
        );
        assert_eq!(
            fs::read(work.0.join("disc.iso")).unwrap(),
            b"previous image"
        );
        work.no_staging();
    }
}

#[test]
fn iso_commit_failure_does_not_commit_the_authored_tree() {
    let work = Directory::new();
    fs::create_dir(work.0.join("disc.iso")).unwrap();
    fs::write(work.0.join("disc.iso/keep"), b"existing directory").unwrap();
    assert!(command::run(&work.arguments(1, 2), &mut NoMenu, &mut Silent).is_err());
    assert!(!work.0.join("disc").exists());
    assert_eq!(
        fs::read(work.0.join("disc.iso/keep")).unwrap(),
        b"existing directory"
    );
    work.no_staging();
}

struct CommitFailure {
    destination: PathBuf,
    injected: bool,
}
impl Callbacks for CommitFailure {
    fn emit(&mut self, _: i32, _: &str) {}
    fn cancelled(&mut self) -> bool {
        false
    }
    fn progress(&mut self, completed: u64, total: u64) {
        if !self.injected && completed == total {
            fs::create_dir(&self.destination).unwrap();
            fs::write(
                self.destination.join("keep"),
                b"destination changed during write",
            )
            .unwrap();
            self.injected = true;
        }
    }
}

#[test]
fn final_iso_rename_failure_rolls_back_new_disc_and_restores_empty_output() {
    for existing_empty in [false, true] {
        let work = Directory::new();
        if existing_empty {
            fs::create_dir(work.0.join("disc")).unwrap();
        }
        let mut events = CommitFailure {
            destination: work.0.join("disc.iso"),
            injected: false,
        };
        assert!(command::run(&work.arguments(1, 2), &mut NoMenu, &mut events).is_err());
        assert!(events.injected);
        assert_eq!(work.0.join("disc").exists(), existing_empty);
        if existing_empty {
            assert!(fs::read_dir(work.0.join("disc")).unwrap().next().is_none());
        }
        assert_eq!(
            fs::read(work.0.join("disc.iso/keep")).unwrap(),
            b"destination changed during write"
        );
        work.no_staging();
    }
}

#[test]
fn overlapping_output_paths_and_source_replacement_fail_without_modifying_inputs() {
    for extra in [vec!["--iso=disc/nested.iso"], vec!["-D", "disc/temp"]] {
        let work = Directory::new();
        let mut arguments = work.arguments(1, 2);
        // Avoid changing the process working directory in parallel test threads.
        if extra.len() == 1 {
            arguments.push(format!(
                "--iso={}",
                work.0.join("disc/nested.iso").display()
            ));
        } else {
            arguments.extend(["-D".into(), work.0.join("disc/temp").display().to_string()]);
        }
        assert!(command::run(&arguments, &mut NoMenu, &mut Silent).is_err());
        assert!(!work.0.join("disc").exists());
        work.no_staging();
    }
    let work = Directory::new();
    let input = work.0.join("音楽 한글.wav");
    let original = fs::read(&input).unwrap();
    let mut arguments = work.arguments(1, 2);
    arguments.push(format!("--iso={}", input.display()));
    assert!(command::run(&arguments, &mut NoMenu, &mut Silent).is_err());
    assert_eq!(fs::read(input).unwrap(), original);
    assert!(!work.0.join("disc").exists());
    work.no_staging();
}

#[test]
fn cancelled_iso_can_be_retried_with_the_same_arguments_and_empty_output() {
    let work = Directory::new();
    fs::create_dir(work.0.join("disc")).unwrap();
    fs::write(work.0.join("disc.iso"), b"previous image").unwrap();
    let arguments = work.arguments(1, 3);
    let mut events = Cancel {
        trigger: "Writing ISO image",
        iso: false,
        cancel: false,
        after_progress: true,
    };
    assert!(command::run(&arguments, &mut NoMenu, &mut events).is_err());
    assert!(fs::read_dir(work.0.join("disc")).unwrap().next().is_none());
    assert_eq!(
        fs::read(work.0.join("disc.iso")).unwrap(),
        b"previous image"
    );
    work.no_staging();
    command::run(&arguments, &mut NoMenu, &mut Silent).unwrap();
    assert!(work.0.join("disc/AUDIO_TS/ATS_01_1.AOB").is_file());
    assert_ne!(
        fs::read(work.0.join("disc.iso")).unwrap(),
        b"previous image"
    );
    work.no_staging();
}

#[cfg(windows)]
#[test]
fn locked_previous_iso_is_preserved_and_final_commit_rolls_back() {
    use std::os::windows::fs::OpenOptionsExt;
    struct Lock {
        destination: PathBuf,
        file: Option<fs::File>,
    }
    impl Callbacks for Lock {
        fn emit(&mut self, _: i32, _: &str) {}
        fn cancelled(&mut self) -> bool {
            false
        }
        fn progress(&mut self, completed: u64, total: u64) {
            if self.file.is_none() && completed == total {
                // Allow readers while withholding FILE_SHARE_DELETE: final
                // atomic replacement must fail without modifying the old ISO.
                self.file = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(&self.destination)
                        .unwrap(),
                );
            }
        }
    }
    let work = Directory::new();
    let image = work.0.join("disc.iso");
    fs::write(&image, b"previous image").unwrap();
    let mut events = Lock {
        destination: image.clone(),
        file: None,
    };
    assert!(command::run(&work.arguments(1, 3), &mut NoMenu, &mut events).is_err());
    assert!(events.file.is_some());
    assert_eq!(fs::read(image).unwrap(), b"previous image");
    assert!(!work.0.join("disc").exists());
    work.no_staging();
    drop(events);
    command::run(&work.arguments(1, 3), &mut NoMenu, &mut Silent).unwrap();
    work.no_staging();
}
