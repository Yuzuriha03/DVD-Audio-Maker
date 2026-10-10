use dvda_author::{
    amg::{self, Group, Layout, Menu, Options, VideoLink},
    asvs,
    atsi::{StillPicture, Title, Track},
    samg,
};

fn track(index: u32) -> Track {
    samg::Track {
        mlp: true,
        channels: 2,
        bits: 16,
        rate: 96000,
        channel_assignment: 1,
        first_pts: 98 + index * 11325,
        pts_length: 11325,
        first_sector: 17 * index,
        last_sector: 17 * index + 16,
    }
    .into()
}
fn groups() -> Vec<Group> {
    vec![
        Group {
            titles: vec![Title {
                tracks: vec![track(0), track(1)]
            }],
            atsi_sectors: 2
        };
        2
    ]
}
fn options() -> Options {
    Options {
        provider: "Provided by dvda-author GPLv3".into(),
        ..Options::default()
    }
}

#[test]
fn menuless_and_multigroup_tables_match_c() {
    let mut g = groups();
    g.truncate(1);
    g[0].titles[0].tracks.truncate(1);
    assert_eq!(
        amg::encode(&g, &Layout::default(), &options())
            .unwrap()
            .bytes,
        include_bytes!("fixtures/manager/single.ifo").as_slice()
    );
    let mut g = groups();
    g[1].titles = vec![
        Title {
            tracks: vec![track(0)],
        },
        Title {
            tracks: vec![track(1)],
        },
    ];
    assert_eq!(
        amg::encode(&g, &Layout::default(), &options())
            .unwrap()
            .bytes,
        include_bytes!("fixtures/manager/multigroup.ifo").as_slice()
    );
}

#[test]
fn pal_and_ntsc_full_menu_pgci_match_c() {
    let mut o = options();
    o.menu = Some(Menu {
        page_sectors: vec![17, 17],
        ..Menu::default()
    });
    let layout = Layout {
        top_vob_sectors: 32,
        ..Layout::default()
    };
    let actual = amg::encode(&groups(), &layout, &o).unwrap();
    assert_eq!(actual.sectors, 4);
    assert_eq!(
        actual.bytes,
        include_bytes!("fixtures/manager/menu-pal-two.ifo").as_slice()
    );
    let pages = vec![17, 17, 23, 29, 31, 37, 41, 43, 47];
    let layout = Layout {
        top_vob_sectors: pages.iter().map(|&s| s - 1).sum(),
        ..Layout::default()
    };
    o.autoplay = true;
    o.menu = Some(Menu {
        page_sectors: pages,
        video_attribute: 0x43,
        duration: [1, 2, 3],
        looped: true,
        ..Menu::default()
    });
    let actual = amg::encode(&groups(), &layout, &o).unwrap();
    assert_eq!(actual.sectors, 5);
    assert_eq!(
        actual.bytes,
        include_bytes!("fixtures/manager/menu-ntsc-nine.ifo").as_slice()
    );
}

#[test]
fn playlists_video_links_and_text_match_c() {
    let o = Options {
        playlist_groups: vec![1],
        video_links: vec![VideoLink {
            titleset: 1,
            pts_length: 90000,
            relative_sector: 800,
        }],
        ..options()
    };
    assert_eq!(
        amg::encode(&groups(), &Layout::default(), &o)
            .unwrap()
            .bytes,
        include_bytes!("fixtures/manager/playlist-video.ifo").as_slice()
    );
    let mut g = groups();
    g.truncate(1);
    let o = Options {
        text: Some(vec![Some("First".into()), Some("Second".into())]),
        ..options()
    };
    assert_eq!(
        amg::encode(&g, &Layout::default(), &o).unwrap().bytes,
        include_bytes!("fixtures/manager/text.ifo").as_slice()
    );
}

fn still_groups() -> Vec<Vec<Title>> {
    let title = |pictures| {
        let mut t = track(0);
        t.pictures.resize(pictures, StillPicture::default());
        Title { tracks: vec![t] }
    };
    vec![vec![title(2), title(0), title(1)], vec![title(1)]]
}

#[test]
fn asvs_pal_ntsc_global_bases_and_relative_offsets_match_c() {
    for (attr, expected) in [
        (
            0x53,
            include_bytes!("fixtures/manager/asvs-pal.ifo").as_slice(),
        ),
        (
            0x43,
            include_bytes!("fixtures/manager/asvs-ntsc.ifo").as_slice(),
        ),
    ] {
        let o = asvs::Options {
            video_attribute: attr,
            ..asvs::Options::default()
        };
        let actual = asvs::encode(&still_groups(), &[23, 25, 17, 12], &o).unwrap();
        assert_eq!(actual.bytes, expected);
        assert_eq!(actual.vob_sectors, 77);
        assert_eq!(actual.picture_titles, 3);
    }
}

#[test]
fn large_title_lists_get_separate_tables_instead_of_c_overlap() {
    let g = Group {
        titles: (0..99)
            .map(|i| Title {
                tracks: vec![track(i)],
            })
            .collect(),
        atsi_sectors: 4,
    };
    let actual = amg::encode(&vec![g; 9], &Layout::default(), &options()).unwrap();
    assert_eq!(actual.sectors, 15);
    assert_eq!(
        u32::from_be_bytes(actual.bytes[0xc8..0xcc].try_into().unwrap()),
        8
    );
    assert_eq!(
        u16::from_be_bytes(actual.bytes[0x800..0x802].try_into().unwrap()),
        891
    );
    assert_eq!(
        u16::from_be_bytes(actual.bytes[0x4000..0x4002].try_into().unwrap()),
        891
    );
    assert_eq!(
        &actual.bytes[0x800..0x800 + 4 + 14 * 891],
        &actual.bytes[0x4000..0x4000 + 4 + 14 * 891]
    );
}

#[test]
fn grouping_is_gapless_and_respects_codec_and_explicit_boundaries() {
    let mut tracks = vec![track(0), track(1), track(2), track(3)];
    tracks[2].audio.mlp = false;
    tracks[3].audio.mlp = false;
    let titles = amg::group_tracks(tracks, &[false, false, false, true]).unwrap();
    assert_eq!(
        titles.iter().map(|t| t.tracks.len()).collect::<Vec<_>>(),
        [2, 1, 1]
    );
}

#[test]
fn menu_and_text_tables_have_distinct_advertised_addresses() {
    let mut g = groups();
    g.truncate(1);
    let o = Options {
        menu: Some(Menu {
            page_sectors: vec![17; 9],
            ..Menu::default()
        }),
        text: Some(vec![Some("First".into()), Some("Second".into())]),
        ..options()
    };
    let actual = amg::encode(&g, &Layout::default(), &o).unwrap();
    assert_eq!(actual.sectors, 13);
    let menu = u32::from_be_bytes(actual.bytes[0xcc..0xd0].try_into().unwrap()) as usize * 2048;
    let text = u32::from_be_bytes(actual.bytes[0xd4..0xd8].try_into().unwrap()) as usize * 2048;
    assert_eq!(menu, 3 * 2048);
    assert_eq!(text, 5 * 2048);
    assert_eq!(&actual.bytes[menu..menu + 4], &[0, 1, 0, 0]);
    assert_eq!(&actual.bytes[text..text + 12], b"DVDATXTDT-MG");
}

#[test]
fn bounds_fail_before_serializing_truncated_fields() {
    let mut g = groups();
    g[0].titles[0].tracks[0].audio.pts_length = u32::MAX;
    assert!(amg::encode(&g, &Layout::default(), &options()).is_err());
    let o = Options {
        menu: Some(Menu {
            page_sectors: vec![1],
            ..Menu::default()
        }),
        ..options()
    };
    assert!(amg::encode(&groups(), &Layout::default(), &o).is_err());
    assert!(asvs::encode(&still_groups(), &[23, 25, 17], &asvs::Options::default()).is_err());
    let mut t = track(0);
    t.pictures.resize(2, StillPicture::default());
    assert!(
        asvs::encode(
            &[vec![Title { tracks: vec![t] }]],
            &[65536, 1],
            &asvs::Options::default()
        )
        .is_err()
    );
}
