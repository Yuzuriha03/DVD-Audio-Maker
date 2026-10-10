use dvda_author::{
    Callbacks, Silent,
    menu::{self, Backend, Options, Target, Track},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dvda-menu-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, b"fixture image").unwrap();
        path
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct FixtureBackend {
    images: Vec<Vec<String>>,
    pages: Vec<PathBuf>,
    still_count: usize,
    fail_spu: bool,
    navigation_count: usize,
}
fn program(norm: &str, still: bool) -> Vec<u8> {
    let mut data = vec![255; 4096];
    data[..4].copy_from_slice(&[0, 0, 1, 0xba]);
    if still {
        data[0x400..0x404].copy_from_slice(&[0, 0, 1, 0xbf]);
    }
    let height = if norm == "ntsc" { 480u16 } else { 576u16 };
    data[2100..2104].copy_from_slice(&[0, 0, 1, 0xb3]);
    data[2104..2112].copy_from_slice(&[
        0x2d,
        (height >> 8) as u8,
        height as u8,
        if norm == "ntsc" { 0x24 } else { 0x23 },
        0,
        0,
        0,
        0,
    ]);
    data[2112..2122].copy_from_slice(&[0, 0, 1, 0xb5, 0x10, 0, 0, 0, 0, 0]);
    data[2122..2132].copy_from_slice(&[0, 0, 1, 0xb5, 0x80, 0, 0xc0, 0x40, 0x80, 0]);
    data[4092..].copy_from_slice(&[0, 0, 1, 0xb9]);
    data
}
impl Backend for FixtureBackend {
    fn image_run(&mut self, args: Vec<String>, _: &mut dyn Callbacks) -> Result<(), String> {
        let path = args
            .last()
            .unwrap()
            .strip_prefix("PNG32:")
            .unwrap_or(args.last().unwrap());
        fs::write(path, b"image").map_err(|error| error.to_string())?;
        self.images.push(args);
        Ok(())
    }
    fn write_y4m(
        &mut self,
        _: &Path,
        output: &Path,
        _: &str,
        _: &str,
        _: &mut dyn Callbacks,
    ) -> Result<(), String> {
        fs::write(output, b"y4m").map_err(|error| error.to_string())
    }
    fn create_mpg(
        &mut self,
        _: &Path,
        _: &Path,
        output: &Path,
        norm: &str,
        _: &str,
        still: bool,
        _: &mut dyn Callbacks,
    ) -> Result<(), String> {
        if still {
            self.still_count += 1;
        }
        fs::write(output, program(norm, still)).map_err(|error| error.to_string())
    }
    fn subpictures(
        &mut self,
        xml: &Path,
        input: &Path,
        output: &Path,
        _: &mut dyn Callbacks,
    ) -> Result<(), String> {
        if self.fail_spu {
            return Err("SPU fixture failure".into());
        }
        let document = fs::read_to_string(xml).unwrap();
        assert!(document.contains("<subpictures format="));
        let mut data = fs::read(input).unwrap();
        data.extend([0; 2048]);
        fs::write(output, data).map_err(|error| error.to_string())?;
        self.pages.push(output.into());
        Ok(())
    }
    fn navigation(
        &mut self,
        xml: &Path,
        output: &Path,
        _: &mut dyn Callbacks,
    ) -> Result<(), String> {
        self.navigation_count += 1;
        let document = fs::read_to_string(xml).unwrap();
        assert_eq!(document.matches("<pgc>").count(), self.pages.len());
        let mut joined = Vec::new();
        for page in &self.pages {
            let data = fs::read(page).unwrap();
            joined.extend(&data[..data.len() - 2048]);
        }
        fs::write(output.join("AUDIO_TS/AUDIO_TS.VOB"), joined).map_err(|error| error.to_string())
    }
}
fn tracks(count: usize) -> Vec<Track> {
    (0..count)
        .map(|index| Track {
            name: format!("Track {}", index + 1),
        })
        .collect()
}

#[test]
fn album_geometry_and_track_targets_match_c_layout() {
    let mut options = Options::default();
    options.parse_option("topmenu", None).unwrap();
    options.parse_option("nmenus", Some("3")).unwrap();
    options
        .parse_option("screentext", Some("DISC=Album A=a,b:Album B=c:Album C=d,e"))
        .unwrap();
    let pages = menu::plan(&[tracks(3), tracks(2)], &options).unwrap();
    assert_eq!(pages[0].buttons[0].rectangle, [33, 128, 708, 208]);
    assert_eq!(pages[0].buttons[0].baseline, [44, 170]);
    assert_eq!(
        pages[1].buttons[0].target,
        Target::Track { group: 1, track: 3 }
    );
    assert_eq!(
        pages[2].buttons[0].target,
        Target::Track { group: 2, track: 1 }
    );
    assert_eq!(pages[0].buttons.last().unwrap().target, Target::Menu(2));
    assert_eq!(pages[1].buttons[1].target, Target::Menu(3));
    assert_eq!(pages[1].buttons[2].target, Target::Menu(1));
    assert_eq!(pages[2].buttons.last().unwrap().target, Target::Menu(2));
    let spu = menu::subpicture_xml(
        &pages[0],
        Path::new("image & <x>.png"),
        Path::new("highlight.png"),
        Path::new("select.png"),
    )
    .unwrap();
    assert!(spu.contains("image &amp; &lt;x&gt;.png"));
    let nav = menu::navigation_xml(
        &pages,
        &["p1.vob".into(), "p2.vob".into(), "p3.vob".into()],
        &options,
    )
    .unwrap();
    assert_eq!(
        nav.matches("<button ").count(),
        pages.iter().map(|page| page.buttons.len()).sum::<usize>()
    );
    assert!(nav.contains("jump group 2 track 1;"));
}

#[test]
fn two_index_pages_keep_index_album_blocks_separate_in_pal_and_ntsc() {
    for norm in ["pal", "ntsc"] {
        let labels = (1..=13).map(|n| format!("Album {n}")).collect::<Vec<_>>();
        let content = (1..=13)
            .map(|n| format!("Album {n}=Track {n}"))
            .collect::<Vec<_>>()
            .join(":");
        let text = format!(
            "DISC=Index1={}:Index2={}:{}",
            labels[..12].join(","),
            labels[12],
            content
        );
        let mut options = Options::default();
        options.parse_option("norm", Some(norm)).unwrap();
        options.parse_option("index-pages", Some("2")).unwrap();
        options.parse_option("screentext", Some(&text)).unwrap();
        let pages = menu::plan(&[tracks(6), tracks(7)], &options).unwrap();
        assert_eq!(pages.len(), 15);
        assert_eq!(pages[0].buttons[11].target, Target::Menu(14));
        assert_eq!(pages[0].buttons[12].target, Target::Menu(2));
        assert_eq!(pages[1].buttons[0].target, Target::Menu(15));
        assert_eq!(pages[1].buttons[1].target, Target::Menu(1));
        assert_eq!(pages[2].buttons[1].target, Target::Menu(4));
        assert_eq!(pages[2].buttons[2].target, Target::Menu(1));
        assert_eq!(pages[14].buttons[1].target, Target::Menu(14));
        assert_eq!(pages[14].buttons[2].target, Target::Menu(1));
        assert!(
            pages
                .iter()
                .flat_map(|page| &page.buttons)
                .all(|b| b.rectangle[3] < options.height())
        );
    }
}

#[test]
fn complete_pipeline_accounts_for_all_pages_and_still_pictures() {
    let work = Workspace::new();
    let first = work.file("first image.png");
    let second = work.file("second image.png");
    let mut options = Options::default();
    options
        .parse_option(
            "screentext",
            Some("DISC=INDEX=First,Second:First=a,b:Second=c"),
        )
        .unwrap();
    options.parse_option("nmenus", Some("3")).unwrap();
    options.parse_option("index-pages", Some("1")).unwrap();
    options.parse_option("norm", Some("ntsc")).unwrap();
    let list = work.0.join("covers.txt");
    fs::write(
        &list,
        format!("{}\n{}\n", first.display(), second.display()),
    )
    .unwrap();
    options.index_covers = Some(list);
    options.still_pics = Some(format!(
        "{};;{},{}",
        first.display(),
        second.display(),
        first.display()
    ));
    let mut backend = FixtureBackend::default();
    let output = menu::author(
        &work.0.join("disc/AUDIO_TS"),
        &work.0.join("temporary"),
        &[tracks(2), tracks(1)],
        &options,
        &mut backend,
        &mut Silent,
    )
    .unwrap();
    let menu = output.top_menu.unwrap();
    assert_eq!(menu.page_sectors, [3, 3, 3]);
    assert_eq!(menu.top_vob_sectors, 6);
    assert_eq!(menu.video_attribute, 0x43);
    let still = output.still.unwrap();
    assert_eq!(still.picture_sectors, [3, 3, 3]);
    assert_eq!(still.still_vob_sectors, 9);
    assert_eq!(still.video_attribute, 0x43);
    assert_eq!(backend.still_count, 3);
    assert_eq!(output.track_pictures[0][0].len(), 1);
    assert!(output.track_pictures[0][1].is_empty());
    assert_eq!(output.track_pictures[1][0].len(), 2);
    let stream = fs::read(work.0.join("disc/AUDIO_TS/AUDIO_SV.VOB")).unwrap();
    for picture in stream.chunks(6144) {
        assert_eq!(&picture[4096..4100], [0, 0, 1, 0xb9]);
        assert_eq!(&picture[0x400..0x404], [255; 4]);
        assert_ne!(picture[2117] & 8, 0);
    }
}

#[test]
fn still_only_has_no_top_menu_and_empty_track_does_not_encode_a_picture() {
    let work = Workspace::new();
    let picture = work.file("picture.png");
    let mut options = Options::default();
    options
        .parse_option("stillpics", Some(&format!("{};", picture.display())))
        .unwrap();
    let mut backend = FixtureBackend::default();
    let result = menu::author(
        &work.0.join("disc/AUDIO_TS"),
        &work.0.join("tmp"),
        &[tracks(2)],
        &options,
        &mut backend,
        &mut Silent,
    )
    .unwrap();
    assert!(result.top_menu.is_none());
    assert_eq!(result.still.unwrap().picture_sectors.len(), 1);
    assert_eq!(backend.navigation_count, 0);
    assert_eq!(backend.still_count, 1);
    assert!(!work.0.join("disc/AUDIO_TS/AUDIO_TS.VOB").exists());
}

#[test]
fn failure_stops_before_navigation_and_disabled_menu_needs_no_backend() {
    let work = Workspace::new();
    let groups = [tracks(2)];
    let mut backend = FixtureBackend::default();
    let result = menu::author(
        &work.0.join("disc/AUDIO_TS"),
        &work.0.join("tmp"),
        &groups,
        &Options::default(),
        &mut backend,
        &mut Silent,
    )
    .unwrap();
    assert!(result.top_menu.is_none() && result.still.is_none());
    assert!(backend.images.is_empty());
    let options = Options {
        enabled: true,
        ..Options::default()
    };
    backend.fail_spu = true;
    assert!(
        menu::author(
            &work.0.join("disc/AUDIO_TS"),
            &work.0.join("tmp"),
            &groups,
            &options,
            &mut backend,
            &mut Silent
        )
        .unwrap_err()
        .contains("SPU fixture failure")
    );
    assert_eq!(backend.navigation_count, 0);
    assert!(!work.0.join("disc/AUDIO_TS/AUDIO_TS.VOB").exists());
}

#[test]
fn invalid_inputs_are_rejected_before_media_encoding() {
    let mut options = Options::default();
    assert!(!options.parse_option("iso-volume", Some("DISC")).unwrap());
    assert!(options.parse_option("norm", Some("broken")).is_err());
    assert!(options.parse_option("nmenus", Some("256")).is_err());
    options.screen_text = Some("DISC=INDEX=ALBUM:ALBUM=only-one-track".into());
    options.index_pages = 1;
    assert!(menu::plan(&[tracks(2)], &options).is_err());
    let work = Workspace::new();
    let mut backend = FixtureBackend::default();
    let options = Options {
        still_pics: Some("one;two;three".into()),
        ..Options::default()
    };
    assert!(
        menu::author(
            &work.0.join("disc/AUDIO_TS"),
            &work.0.join("tmp"),
            &[tracks(2)],
            &options,
            &mut backend,
            &mut Silent
        )
        .is_err()
    );
    assert!(backend.images.is_empty());
}
