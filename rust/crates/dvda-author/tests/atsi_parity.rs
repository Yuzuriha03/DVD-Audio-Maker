use dvda_author::{
    atsi::{self, Options, StillPicture, Title, Track},
    samg,
};

fn metadata(csv: &str) -> Vec<Title> {
    let mut titles: Vec<Title> = Vec::new();
    for line in csv.lines() {
        let v: Vec<u32> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let title = v[0] as usize;
        if title > titles.len() {
            titles.push(Title { tracks: Vec::new() });
        }
        titles[title - 1].tracks.push(Track {
            audio: samg::Track {
                mlp: v[1] != 0,
                channels: v[2] as u8,
                bits: v[3] as u8,
                rate: v[4],
                channel_assignment: v[5] as u8,
                first_pts: v[6],
                pts_length: v[7],
                first_sector: v[8],
                last_sector: v[9],
            },
            downmix_table_rank: v[10] as u8,
            pictures: Vec::new(),
        });
    }
    titles
}

#[test]
fn complete_files_match_frozen_author_for_mlp_lpcm_and_mixed_titles() {
    let cases: &[(&str, &[u8])] = &[
        (
            include_str!("fixtures/atsi/original-stereo-mlp.csv"),
            include_bytes!("fixtures/atsi/original-stereo-mlp.ifo"),
        ),
        (
            include_str!("fixtures/atsi/stereo-mlp-two.csv"),
            include_bytes!("fixtures/atsi/stereo-mlp-two.ifo"),
        ),
        (
            include_str!("fixtures/atsi/stereo-mlp-99.csv"),
            include_bytes!("fixtures/atsi/stereo-mlp-99.ifo"),
        ),
        (
            include_str!("fixtures/atsi/surround-mlp.csv"),
            include_bytes!("fixtures/atsi/surround-mlp.ifo"),
        ),
        (
            include_str!("fixtures/atsi/stereo-lpcm-two.csv"),
            include_bytes!("fixtures/atsi/stereo-lpcm-two.ifo"),
        ),
        (
            include_str!("fixtures/atsi/surround-lpcm.csv"),
            include_bytes!("fixtures/atsi/surround-lpcm.ifo"),
        ),
        (
            include_str!("fixtures/atsi/mixed-three-titles.csv"),
            include_bytes!("fixtures/atsi/mixed-three-titles.ifo"),
        ),
        (
            include_str!("fixtures/atsi/mixed-99-titles.csv"),
            include_bytes!("fixtures/atsi/mixed-99-titles.ifo"),
        ),
    ];
    for (csv, expected) in cases {
        let actual = atsi::encode(&metadata(csv), &Options::default()).unwrap();
        assert_eq!(actual.bytes, *expected);
        assert_eq!(usize::from(actual.sectors) * 2048, expected.len());
    }
}

#[test]
fn custom_downmix_matches_unchanged_c_encoder_including_float_boundaries() {
    let mut titles = metadata(include_str!("fixtures/atsi/surround-mlp.csv"));
    titles[0].tracks[0].downmix_table_rank = 2;
    let options = Options {
        downmix: Some(
            [[
                0.2, 1.4, 4.3, 7.2, 40.1, 40.5, 40.9, 41.2, 42.9, 61.8, 100.0, 0.0,
            ]; 16],
        ),
        ..Options::default()
    };
    let actual = atsi::encode(&titles, &options).unwrap();
    assert_eq!(
        actual.bytes,
        include_bytes!("fixtures/atsi/custom-downmix.ifo").as_slice()
    );
}

#[test]
fn per_track_still_offsets_match_real_author() {
    let mut titles = metadata(include_str!("fixtures/atsi/stereo-mlp-two.csv"));
    for track in &mut titles[0].tracks {
        track.pictures.push(StillPicture::default());
    }
    let actual = atsi::encode(&titles, &Options::default()).unwrap();
    assert_eq!(
        actual.bytes,
        include_bytes!("fixtures/atsi/stills-two-tracks.ifo").as_slice()
    );
    assert_eq!(actual.still_picture_titles, 1);
}

fn three_titles() -> Vec<Title> {
    metadata(
        "1,1,2,16,96000,1,98,11325,0,16,0\n2,1,2,16,96000,1,98,11325,17,33,0\n3,1,2,16,96000,1,98,11325,34,50,0\n",
    )
}

#[test]
fn empty_picture_title_uses_next_options_as_in_c() {
    let mut titles = three_titles();
    titles[0].tracks[0].pictures.push(StillPicture::default());
    titles[2].tracks[0].pictures.push(StillPicture {
        manual: true,
        onset_seconds: Some(2),
        start_effect: 3,
        end_effect: 4,
    });
    let actual = atsi::encode(&titles, &Options::default()).unwrap();
    assert_eq!(
        actual.bytes,
        include_bytes!("fixtures/atsi/manual-empty.ifo").as_slice()
    );
    assert_eq!(actual.still_picture_titles, 2);
}

#[test]
fn leading_empty_still_title_matches_c() {
    let mut titles = three_titles();
    titles.pop();
    titles[1].tracks[0].pictures.push(StillPicture {
        manual: true,
        ..StillPicture::default()
    });
    let actual = atsi::encode(&titles, &Options::default()).unwrap();
    assert_eq!(
        actual.bytes,
        include_bytes!("fixtures/atsi/stills-leading-empty.ifo").as_slice()
    );
}

#[test]
fn overflows_and_malformed_layouts_are_errors() {
    let mut titles = metadata(include_str!("fixtures/atsi/stereo-mlp-two.csv"));
    titles[0].tracks[0].audio.pts_length = u32::MAX;
    assert!(
        atsi::encode(&titles, &Options::default())
            .unwrap_err()
            .contains("PTS length")
    );
    titles[0].tracks[0].audio.pts_length = 1;
    titles[0].tracks[1].audio.last_sector = u32::MAX;
    assert!(
        atsi::encode(&titles, &Options::default())
            .unwrap_err()
            .contains("last sector")
    );
    titles[0].tracks[1].audio.last_sector = 33;
    titles[0].tracks[0].downmix_table_rank = 17;
    assert!(atsi::encode(&titles, &Options::default()).is_err());
    titles[0].tracks[0].downmix_table_rank = 0;
    titles[0].tracks[1].audio.rate = 48000;
    assert!(
        atsi::encode(&titles, &Options::default())
            .unwrap_err()
            .contains("split the title")
    );
    titles[0].tracks[1].audio.rate = 96000;
    titles[0].tracks[0]
        .pictures
        .resize(256, StillPicture::default());
    assert!(atsi::encode(&titles, &Options::default()).is_err());
    titles[0].tracks[0].pictures = vec![StillPicture {
        onset_seconds: Some(u16::MAX),
        ..StillPicture::default()
    }];
    assert!(
        atsi::encode(&titles, &Options::default())
            .unwrap_err()
            .contains("still onset")
    );
    assert!(atsi::encode(&[], &Options::default()).is_err());
    assert!(atsi::downmix_coefficient(f32::NAN).is_err());
    assert!(atsi::downmix_coefficient(f32::INFINITY).is_err());
}
