//! Audio titleset information (ATS_XX_0.IFO/BUP), ported from `atsi2.c`.
//!
//! `encode_checked` is used by the Rust author and indexes attributes by the
//! actual format and title. `encode` preserves frozen C indexing for differential
//! fixtures, including C's mixed-format lookup defects.

use crate::samg;

/// One still-picture entry. `None` onset uses one-second intervals within a track.
#[derive(Clone, Copy, Debug, Default)]
pub struct StillPicture {
    pub manual: bool,
    pub onset_seconds: Option<u16>,
    pub start_effect: u8,
    pub end_effect: u8,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub audio: samg::Track,
    /// Zero disables downmix; 1..=16 selects a matrix.
    pub downmix_table_rank: u8,
    pub pictures: Vec<StillPicture>,
}

impl From<samg::Track> for Track {
    fn from(audio: samg::Track) -> Self {
        Self {
            audio,
            downmix_table_rank: 0,
            pictures: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Title {
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Six speaker pairs, in C order: Lf, Rf, C, S, Rs, LFE; left then right.
    /// Values are attenuation in dB; 100 means off. None uses C's default matrix.
    pub downmix: Option<[[f32; 12]; 16]>,
    /// Enables still records even when this group only reuses earlier pictures.
    /// The value is the number of earlier titles with their own pictures (ASVS
    /// rank). Supply the previous group's `Encoded::still_picture_titles`.
    pub still_picture_titles: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct Encoded {
    /// Write these same bytes to IFO and BUP.
    pub bytes: Vec<u8>,
    pub sectors: u8,
    pub still_picture_titles: u8,
}

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn bits_code(bits: u8) -> Result<u8, String> {
    match bits {
        16 => Ok(0),
        20 => Ok(1),
        24 => Ok(2),
        _ => Err("ATSI requires 16, 20 or 24 bits".into()),
    }
}

fn rate_code(rate: u32) -> Result<u8, String> {
    match rate {
        48000 => Ok(0),
        96000 => Ok(1),
        192000 => Ok(2),
        44100 => Ok(8),
        88200 => Ok(9),
        176400 => Ok(10),
        _ => Err("Unsupported ATSI sample rate".into()),
    }
}

/// Reproduces the C coefficient conversion, including its truncation in 0..1.4
/// dB and discontinuous high-attenuation ranges. NaN/infinity are rejected instead
/// of relying on undefined float-to-integer conversions.
pub fn downmix_coefficient(db: f32) -> Result<u8, String> {
    if !db.is_finite() || !(0.0..=100.0).contains(&db) {
        return Err("Downmix attenuation must be finite and in 0..=100 dB".into());
    }
    if db <= 1.4 {
        return Ok((db as u8) * 5);
    }
    // `db` is a C float, promoted to double by the decimal literals.
    let db = f64::from(db);
    for (limit, base) in [
        (4.3, 0x15),
        (7.2, 0x23),
        (10.1, 0x31),
        (13.2, 0x40),
        (16.1, 0x4e),
        (19.0, 0x5c),
        (22.1, 0x6b),
        (25.0, 0x79),
        (27.9, 0x87),
        (31.0, 0x96),
        (33.9, 0xa4),
        (36.8, 0xb2),
        (39.7, 0xc0),
    ] {
        if db <= limit {
            return Ok((f64::from(base) - (limit - db) / 0.2) as u8);
        }
    }
    if db <= 41.2 {
        // Equality here is intentionally double equality, as in the C source.
        return Ok(if db == 40.1 {
            0xc8
        } else if db == 40.5 {
            0xc9
        } else if db == 40.9 {
            0xca
        } else {
            (199.0 - (41.2 - db) / 0.2) as u8
        });
    }
    if db <= 42.9 {
        return Ok((207.0 - (42.9 - db) / 0.4) as u8);
    }
    if db <= 61.8 {
        return Ok((254.0 - (61.8 - db) / 0.4) as u8);
    }
    Ok(if db == 100.0 { 0xff } else { 0 })
}

fn same_format(a: &samg::Track, b: &samg::Track) -> bool {
    a.rate == b.rate && a.bits == b.bits && a.channels == b.channels
}

/// Encode one audio titleset, with 1..=99 tracks and at most eight C audio-format
/// identities. Titles must be homogeneous in rate/width/channels, as prepared by
/// C's `create_tracktables`; the caller supplies title boundaries and AOB ranges.
/// Overflow and malformed metadata are errors rather than truncated table fields.
pub fn encode(titles: &[Title], options: &Options) -> Result<Encoded, String> {
    encode_impl(titles, options, false)
}

/// Production encoder: codec and channel assignment are part of the audio
/// format identity, and every title/track points to its own attributes.
pub fn encode_checked(titles: &[Title], options: &Options) -> Result<Encoded, String> {
    encode_impl(titles, options, true)
}

fn encode_impl(titles: &[Title], options: &Options, checked: bool) -> Result<Encoded, String> {
    let same_format = |a: &samg::Track, b: &samg::Track| {
        same_format(a, b)
            && (!checked || (a.mlp == b.mlp && a.channel_assignment == b.channel_assignment))
    };
    let tracks: Vec<&Track> = titles.iter().flat_map(|t| &t.tracks).collect();
    if titles.is_empty()
        || tracks.is_empty()
        || tracks.len() > 99
        || titles.iter().any(|t| t.tracks.is_empty())
    {
        return Err("ATSI requires nonempty titles and 1..=99 tracks".into());
    }
    let has_pictures = tracks.iter().any(|t| !t.pictures.is_empty());
    let stills = options.still_picture_titles.is_some() || has_pictures;
    let mut picture_titles = options.still_picture_titles.unwrap_or(0);
    for track in &tracks {
        bits_code(track.audio.bits)?;
        rate_code(track.audio.rate)?;
        if !(1..=6).contains(&track.audio.channels) || track.audio.channel_assignment > 20 {
            return Err("ATSI requires 1..=6 channels and a channel assignment in 0..=20".into());
        }
        if track.downmix_table_rank > 16 || track.audio.first_sector > track.audio.last_sector {
            return Err("Invalid ATSI downmix rank or AOB sector range".into());
        }
    }
    for title in titles {
        if title
            .tracks
            .iter()
            .any(|t| !same_format(&t.audio, &title.tracks[0].audio))
        {
            return Err("ATSI title mixes rate, width or channel count; split the title".into());
        }
        if title.tracks.iter().map(|t| t.pictures.len()).sum::<usize>() > 255 {
            return Err("ATSI title exceeds the one-byte picture index (255 pictures)".into());
        }
    }
    let mut formats: Vec<samg::Track> = Vec::new();
    for title in titles {
        let audio = title.tracks[0].audio;
        if !formats.iter().any(|f| same_format(f, &audio)) {
            formats.push(audio);
        }
    }
    if formats.len() > 8 {
        return Err("ATSI supports at most eight audio formats".into());
    }

    let mut bytes = vec![0; 0x800 + 8 + titles.len() * 8];
    bytes[..12].copy_from_slice(b"DVDAUDIO-ATS");
    put16(&mut bytes, 0x20, 0x12);
    put32(&mut bytes, 0x80, 0x7ff);
    put32(&mut bytes, 0xcc, 1);
    let mut multichannel = false;
    for (index, format) in formats.iter().enumerate() {
        let offset = 0x100 + index * 16;
        put16(&mut bytes, offset, u16::from(format.mlp) << 8);
        // Frozen C uses files[index] for these two fields, not formats[index].
        let source = if checked {
            *format
        } else {
            tracks[index].audio
        };
        let surround = source.channels > 2;
        multichannel |= surround;
        let bits = bits_code(format.bits)?;
        let rate = rate_code(format.rate)?;
        bytes[offset + 2] = if surround {
            bits * 0x11
        } else {
            bits * 0x10 + 0xf
        };
        bytes[offset + 3] = if surround {
            rate * 0x11
        } else {
            rate * 0x10 + 0xf
        };
        bytes[offset + 4] = source.channel_assignment;
    }
    if multichannel {
        for rank in 0..16 {
            let offset = 0x180 + rank * 18 + 2;
            if let Some(table) = &options.downmix {
                for (channel, db) in table[rank].iter().enumerate() {
                    bytes[offset + channel] = downmix_coefficient(*db)?;
                }
            } else {
                bytes[offset..offset + 12].copy_from_slice(&[
                    0x1e, 0xff, 0xff, 0x1e, 0x2d, 0x2d, 0x3c, 0xff, 0xff, 0x3c, 0x4b, 0x4b,
                ]);
            }
        }
    }
    put16(&mut bytes, 0x800, titles.len() as u16);
    let all_pictures: Vec<&StillPicture> = tracks.iter().flat_map(|t| &t.pictures).collect();
    let mut preceding_group_pictures = 0;
    for (index, title) in titles.iter().enumerate() {
        let entry = 0x808 + index * 8;
        put16(&mut bytes, entry, 0x8000 + (index as u16 + 1) * 0x100);
        // C writes a second u16 one byte after the first, retaining only 0x81..
        // from the first field and replacing its second byte with this flag.
        let flag_audio = if checked {
            title.tracks[0].audio
        } else {
            tracks[index].audio
        };
        let flag = if flag_audio.mlp {
            if flag_audio.channels > 2 { 0x101 } else { 1 }
        } else {
            0x100
        };
        put16(&mut bytes, entry + 1, flag);
        let start = bytes.len();
        put32(&mut bytes, entry + 4, (start - 0x800) as u32);
        let count = title.tracks.len();
        bytes.resize(start + 16 + 32 * count, 0);
        bytes[start + 2] = count as u8;
        bytes[start + 3] = count as u8;
        let length = title
            .tracks
            .iter()
            .try_fold(0_u32, |length, t| length.checked_add(t.audio.pts_length))
            .ok_or("ATSI title PTS length exceeds 32 bits")?;
        put32(&mut bytes, start + 4, length);
        put16(&mut bytes, start + 10, 16);
        put16(&mut bytes, start + 12, (16 + 20 * count) as u16);
        if stills {
            put16(&mut bytes, start + 14, (16 + 32 * count) as u16);
        }
        let format = formats
            .iter()
            .position(|f| same_format(f, &title.tracks[0].audio))
            .unwrap();
        for (rank, track) in title.tracks.iter().enumerate() {
            let pts = start + 16 + 20 * rank;
            let mut flags = (format as u16 * 8) << 8;
            if rank == 0 {
                flags |= 0xc000;
            }
            flags |= if (if checked {
                track.audio.channels
            } else {
                tracks[index].audio.channels
            }) < 2
                || track.downmix_table_rank == 0
            {
                0x10
            } else {
                u16::from(track.downmix_table_rank - 1)
            };
            put16(&mut bytes, pts, flags);
            bytes[pts + 4] = rank as u8 + 1;
            put32(&mut bytes, pts + 6, track.audio.first_pts);
            put32(&mut bytes, pts + 10, track.audio.pts_length);
            let sector = start + 16 + 20 * count + 12 * rank;
            bytes[sector] = 1;
            put32(&mut bytes, sector + 4, track.audio.first_sector);
            put32(&mut bytes, sector + 8, track.audio.last_sector);
        }
        if stills {
            let pic_count: usize = title.tracks.iter().map(|t| t.pictures.len()).sum();
            if pic_count > 0 {
                picture_titles = picture_titles
                    .checked_add(1)
                    .ok_or("ASVS picture title rank exceeds 255")?;
            }
            // C suppresses rows until a first picture title exists. Retained for
            // parity; its still-table pointer can then point at an empty table.
            let rows = if picture_titles > 0 { 6 * count } else { 0 };
            let still_start = bytes.len();
            bytes.resize(still_start + rows + 10 * pic_count, 0);
            // C's options[s] can reference the next title's first picture when
            // this title only reuses a preceding picture. Preserve that index.
            let first_picture = all_pictures.get(preceding_group_pictures);
            let manual = first_picture.is_some_and(|p| p.manual);
            let mut preceding = 0;
            for (rank, track) in title.tracks.iter().enumerate() {
                if rows > 0 {
                    let offset = still_start + 6 * rank;
                    bytes[offset] = picture_titles;
                    bytes[offset + 1] = if manual { 4 } else { 0 };
                    let first = 6 * count + 10 * preceding;
                    put16(&mut bytes, offset + 2, first as u16);
                    // A zero-picture track reuses the preceding title/picture.
                    put16(
                        &mut bytes,
                        offset + 4,
                        (first + 10 * track.pictures.len() - 1) as u16,
                    );
                }
                for (pic_rank, picture) in track.pictures.iter().enumerate() {
                    let offset = still_start + rows + 10 * (preceding + pic_rank);
                    bytes[offset] = (preceding + pic_rank + 1) as u8;
                    bytes[offset + 3] = rank as u8 + 1;
                    let onset = match picture.onset_seconds {
                        Some(value) if value != 0 => u32::from(value)
                            .checked_mul(90000)
                            .ok_or("ATSI still onset exceeds 32-bit PTS")?,
                        _ if !manual => pic_rank as u32 * 90000,
                        _ => 0,
                    };
                    put32(&mut bytes, offset + 4, onset);
                    bytes[offset + 8] = picture.start_effect;
                    bytes[offset + 9] = picture.end_effect;
                }
                preceding += track.pictures.len();
            }
            preceding_group_pictures += pic_count;
        }
    }
    let sectors =
        u8::try_from(bytes.len().div_ceil(2048).max(2)).map_err(|_| "ATSI exceeds 255 sectors")?;
    let last = tracks
        .last()
        .unwrap()
        .audio
        .last_sector
        .checked_add(2 * u32::from(sectors))
        .ok_or("ATSI last sector exceeds 32 bits")?;
    let end = (bytes.len() - 0x801) as u32;
    put32(&mut bytes, 0x804, end);
    put32(&mut bytes, 12, last);
    put32(&mut bytes, 28, u32::from(sectors) - 1);
    put32(&mut bytes, 196, u32::from(sectors));
    bytes.resize(usize::from(sectors) * 2048, 0);
    Ok(Encoded {
        bytes,
        sectors,
        still_picture_titles: picture_titles,
    })
}
