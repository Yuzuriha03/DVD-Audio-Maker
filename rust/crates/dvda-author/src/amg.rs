//! Audio manager title tables and complete menu PGCI, ported from `amg2.c`.
//! The two title tables expand independently beyond C's one-sector capacity.
//! Menu cells retain the C producer's dropped trailing sector convention.
//! Optional text follows the menu table; C wrote it over the menu table despite
//! pointing to a later sector. Its historical byte-sized text indices are kept.

use crate::atsi::{Title, Track};

#[derive(Clone, Debug)]
pub struct Group {
    pub titles: Vec<Title>,
    pub atsi_sectors: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub asvs_sectors: u8,
    pub still_vob_sectors: u32,
    pub top_vob_sectors: u32,
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub page_sectors: Vec<u32>,
    pub video_attribute: u8,
    pub duration: [u8; 3],
    pub looped: bool,
    /// Text, highlight, selected foreground, background, as packed Y/Cr/Cb.
    pub palette: [u32; 4],
}

impl Default for Menu {
    fn default() -> Self {
        Self {
            page_sectors: Vec::new(),
            video_attribute: 0x53,
            duration: [0; 3],
            looped: false,
            palette: [0x00eb8080, 0x0051f05a, 0x00808080, 0x00108080],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VideoLink {
    pub titleset: u8,
    pub pts_length: u32,
    pub relative_sector: u32,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub provider: String,
    pub autoplay: bool,
    pub menu: Option<Menu>,
    /// Zero-based indices of real audio groups copied into the title list.
    pub playlist_groups: Vec<usize>,
    pub video_links: Vec<VideoLink>,
    /// C's optional single-group DVDATXTDT-MG table (one byte-string per track).
    pub text: Option<Vec<Option<String>>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            provider: "Provided by dvda-author GPLv3".into(),
            autoplay: false,
            menu: None,
            playlist_groups: Vec::new(),
            video_links: Vec::new(),
            text: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub sectors: u8,
}

fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_be_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_be_bytes());
}
fn bcd(v: u8) -> u8 {
    v / 10 * 16 + v % 10
}
fn add(a: u32, b: u32) -> Result<u32, String> {
    a.checked_add(b)
        .ok_or_else(|| "AMG sector address exceeds 32 bits".into())
}

/// Group by audio attributes and explicit boundaries. A codec change also starts
/// a title: C omitted this test, permitting LPCM and MLP in one playback title.
pub fn group_tracks(tracks: Vec<Track>, forced_new_titles: &[bool]) -> Result<Vec<Title>, String> {
    if tracks.is_empty()
        || tracks.len() > 99
        || (!forced_new_titles.is_empty() && forced_new_titles.len() != tracks.len())
    {
        return Err("A group requires 1..=99 tracks and matching explicit title flags".into());
    }
    let mut titles: Vec<Title> = Vec::new();
    for (index, track) in tracks.into_iter().enumerate() {
        let previous = titles
            .last()
            .and_then(|t| t.tracks.last())
            .map(|t| &t.audio);
        let a = &track.audio;
        let new_title = previous.is_none_or(|p| {
            p.rate != a.rate
                || p.bits != a.bits
                || p.channels != a.channels
                || p.channel_assignment != a.channel_assignment
                || p.mlp != a.mlp
        }) || forced_new_titles.get(index).copied().unwrap_or(false);
        if new_title {
            titles.push(Title { tracks: Vec::new() });
        }
        titles.last_mut().unwrap().tracks.push(track);
    }
    Ok(titles)
}

fn total_titles(groups: &[Group], options: &Options) -> Result<usize, String> {
    if groups.is_empty()
        || groups.len() + options.playlist_groups.len() + options.video_links.len() > 9
    {
        return Err("AMG requires 1..=9 total audio, playlist and video groups".into());
    }
    for group in groups {
        let count: usize = group.titles.iter().map(|t| t.tracks.len()).sum();
        if !(1..=99).contains(&count)
            || group.atsi_sectors < 2
            || group.titles.iter().any(|t| t.tracks.is_empty())
        {
            return Err("Invalid AMG group track count, empty title or ATSI size".into());
        }
    }
    let mut count =
        groups.iter().map(|g| g.titles.len()).sum::<usize>() + options.video_links.len();
    for &group in &options.playlist_groups {
        count += groups
            .get(group)
            .ok_or("Invalid playlist group index")?
            .titles
            .len();
    }
    Ok(count)
}

/// Compute layout before assigning SAMG absolute addresses.
pub fn sector_count(groups: &[Group], options: &Options) -> Result<u8, String> {
    let titles = total_titles(groups, options)?;
    let table_sectors = (4 + 14 * titles).div_ceil(2048);
    let base = 1 + 2 * table_sectors;
    let mut sectors = base;
    if let Some(menu) = &options.menu {
        if menu.page_sectors.is_empty()
            || menu.page_sectors.len() > 255
            || menu.page_sectors.iter().any(|&s| s < 2)
        {
            return Err("AMG menu requires 1..=255 pages of at least two sectors".into());
        }
        if menu.duration[0] > 99 || menu.duration[1] > 59 || menu.duration[2] > 59 {
            return Err("Invalid AMG menu duration".into());
        }
        sectors += (24 + 314 * menu.page_sectors.len()).div_ceil(2048);
    }
    if options.text.is_some() {
        if groups.len() != 1 {
            return Err("DVDATXTDT-MG is only defined by the C author for one group".into());
        }
        // The C allocation reserves eight sectors, retaining parity for its
        // historical byte-sized text indices rather than inventing a format.
        sectors += 8;
    }
    u8::try_from(sectors).map_err(|_| "AMG exceeds 255 sectors".into())
}

fn title_length(title: &Title) -> Result<u32, String> {
    title
        .tracks
        .iter()
        .try_fold(0u32, |sum, t| sum.checked_add(t.audio.pts_length))
        .ok_or_else(|| "AMG title duration exceeds 32-bit PTS".into())
}

fn record(
    bytes: &mut [u8],
    offset: usize,
    tag: u8,
    tracks: u8,
    length: u32,
    target: (u8, u8),
    sector: u32,
) {
    bytes[offset] = tag;
    bytes[offset + 1] = tracks;
    put32(bytes, offset + 4, length);
    bytes[offset + 8] = target.0;
    bytes[offset + 9] = target.1;
    put32(bytes, offset + 10, sector);
}

/// Encode AUDIO_TS.IFO; write the exact same bytes to AUDIO_TS.BUP.
pub fn encode(groups: &[Group], layout: &Layout, options: &Options) -> Result<Encoded, String> {
    let count = total_titles(groups, options)?;
    let sectors = sector_count(groups, options)?;
    let table_sectors = (4 + 14 * count).div_ceil(2048);
    let second = 1 + table_sectors;
    let menu_sector = 1 + 2 * table_sectors;
    let mut bytes = vec![0; usize::from(sectors) * 2048];
    bytes[..12].copy_from_slice(b"DVDAUDIO-AMG");
    let span = add(2 * u32::from(sectors), layout.top_vob_sectors)?;
    put32(&mut bytes, 0xc, span - 1);
    put32(&mut bytes, 0x1c, u32::from(sectors) - 1);
    put16(&mut bytes, 0x20, 0x12);
    put16(&mut bytes, 0x26, 1);
    put16(&mut bytes, 0x28, 1);
    bytes[0x2a] = 1;
    bytes[0x2f] = u8::from(options.autoplay);
    if layout.still_vob_sectors > 0 {
        put32(&mut bytes, 0x30, span);
    }
    bytes[0x3f] = (groups.len() + options.playlist_groups.len() + options.video_links.len()) as u8;
    let provider = options.provider.as_bytes();
    let length = provider.len().min(30);
    bytes[0x40..0x40 + length].copy_from_slice(&provider[..length]);
    put32(&mut bytes, 0x80, 0x7ff);
    put32(
        &mut bytes,
        0xc0,
        if options.menu.is_some() {
            u32::from(sectors)
        } else {
            0
        },
    );
    put32(&mut bytes, 0xc4, 1);
    put32(&mut bytes, 0xc8, second as u32);
    put32(
        &mut bytes,
        0xcc,
        if options.menu.is_some() {
            menu_sector as u32
        } else {
            0
        },
    );
    let text_sector = menu_sector
        + options
            .menu
            .as_ref()
            .map_or(0, |m| (24 + 314 * m.page_sectors.len()).div_ceil(2048));
    put32(
        &mut bytes,
        0xd4,
        if options.text.is_some() {
            text_sector as u32
        } else {
            0
        },
    );
    if let Some(menu) = &options.menu {
        bytes[0x100] = menu.video_attribute;
        put32(&mut bytes, 0x154, 0x10000);
        if menu.duration.iter().any(|&v| v > 0) {
            put32(&mut bytes, 0x15c, 0x18001);
        }
    }
    let mut offsets = Vec::new();
    let mut offset = add(
        add(
            2 * (u32::from(sectors) + u32::from(layout.asvs_sectors)),
            layout.still_vob_sectors,
        )?,
        layout.top_vob_sectors,
    )?;
    for group in groups {
        offsets.push(offset);
        let last = group
            .titles
            .last()
            .unwrap()
            .tracks
            .last()
            .unwrap()
            .audio
            .last_sector;
        offset = add(
            add(offset, add(last, 1)?)?,
            2 * u32::from(group.atsi_sectors),
        )?;
    }
    for (table_index, table) in [2048, second * 2048].into_iter().enumerate() {
        put16(&mut bytes, table, count as u16);
        put16(&mut bytes, table + 2, (3 + 14 * count) as u16);
        let mut index = 0;
        for (g, group) in groups.iter().enumerate() {
            for (t, title) in group.titles.iter().enumerate() {
                let tag =
                    if table_index == 0 && options.menu.is_some() && t + 1 == group.titles.len() {
                        0xc0
                    } else {
                        0x80
                    };
                record(
                    &mut bytes,
                    table + 4 + 14 * index,
                    tag | (g as u8 + 1),
                    title.tracks.len() as u8,
                    title_length(title)?,
                    (g as u8 + 1, t as u8 + 1),
                    offsets[g],
                );
                index += 1;
            }
        }
        for (rank, link) in options.video_links.iter().enumerate() {
            if link.titleset == 0 {
                return Err("Video link titleset number is one-based".into());
            }
            record(
                &mut bytes,
                table + 4 + 14 * index,
                0x40 | (groups.len() + rank + 1) as u8,
                1,
                link.pts_length,
                (link.titleset, 1),
                link.relative_sector,
            );
            // C has a second chapter byte in the padding.
            bytes[table + 4 + 14 * index + 2] = 1;
            index += 1;
        }
        for &g in &options.playlist_groups {
            for (t, title) in groups[g].titles.iter().enumerate() {
                record(
                    &mut bytes,
                    table + 4 + 14 * index,
                    0x80 | (g as u8 + 1),
                    title.tracks.len() as u8,
                    title_length(title)?,
                    (g as u8 + 1, t as u8 + 1),
                    offsets[g],
                );
                index += 1;
            }
        }
    }
    if let Some(menu) = &options.menu {
        write_menu(&mut bytes, menu_sector * 2048, menu)?;
    }
    if let Some(text) = &options.text {
        write_text(
            &mut bytes,
            text_sector * 2048,
            groups[0].titles.iter().map(|t| t.tracks.len()).sum(),
            text,
        )?;
    }
    Ok(Encoded { bytes, sectors })
}

fn write_menu(bytes: &mut [u8], base: usize, menu: &Menu) -> Result<(), String> {
    let count = menu.page_sectors.len();
    put32(bytes, base, 0x10000);
    put32(bytes, base + 4, (23 + 314 * count) as u32);
    put32(bytes, base + 8, 0x656e0080);
    put32(bytes, base + 12, 16);
    put16(bytes, base + 16, count as u16);
    put32(bytes, base + 20, (7 + 314 * count) as u32);
    let mut sector = 0;
    for (page, &size) in menu.page_sectors.iter().enumerate() {
        let directory = base + 24 + 8 * page;
        if page == 0 {
            put32(bytes, directory, 0x82000000);
        }
        put32(bytes, directory + 4, (8 * (count + 1) + 306 * page) as u32);
        let pgc = base + 24 + 8 * count + 306 * page;
        put32(bytes, pgc, 0x101);
        for index in 0..3 {
            bytes[pgc + 4 + index] = bcd(menu.duration[index]);
        }
        bytes[pgc + 7] = 0xc1;
        bytes[pgc + 12] = if menu.duration.iter().any(|&v| v > 0) {
            0x80
        } else {
            0
        };
        put32(bytes, pgc + 28, 0x80000000);
        if page + 1 < count {
            put16(bytes, pgc + 156, page as u16 + 2);
        }
        if page > 0 {
            put16(bytes, pgc + 158, page as u16);
        }
        put16(bytes, pgc + 160, 1);
        for index in 0..16 {
            put32(
                bytes,
                pgc + 164 + 4 * index,
                if index < 4 {
                    menu.palette[index]
                } else {
                    0x108080
                },
            );
        }
        put32(bytes, pgc + 228, 0xec0114);
        put32(bytes, pgc + 232, 0x116012e);
        put32(bytes, pgc + 236, 0x10003);
        put32(bytes, pgc + 240, 0x27);
        if menu.looped {
            put16(bytes, pgc + 252, 0x2004);
            bytes[pgc + 247] = 1;
        }
        put32(bytes, pgc + 276, 0x1000200);
        put16(bytes, pgc + 280, if menu.looped { 1 } else { 0xff00 });
        for index in 0..3 {
            bytes[pgc + 282 + index] = bcd(menu.duration[index]);
        }
        bytes[pgc + 285] = 0xc1;
        put32(bytes, pgc + 286, sector);
        put32(bytes, pgc + 294, sector);
        put32(bytes, pgc + 298, add(sector, size - 2)?);
        put32(bytes, pgc + 302, 0x10001);
        sector = add(sector, size - 1)?;
    }
    Ok(())
}

fn write_text(
    bytes: &mut [u8],
    base: usize,
    count: usize,
    text: &[Option<String>],
) -> Result<(), String> {
    if text.len() < count.max(2) {
        return Err("C text table requires strings for all tracks and two initial entries".into());
    }
    let mut i = base;
    bytes[i..i + 12].copy_from_slice(b"DVDATXTDT-MG");
    i += 12;
    put32(bytes, i, 1);
    i += 8;
    bytes[i..i + 2].copy_from_slice(b"en");
    i += 3;
    bytes[i] = 0x11;
    i += 4;
    bytes[i] = 0x1c;
    i += 6;
    bytes[i] = 0x2a;
    i += 2;
    bytes[i] = 0x32;
    bytes[base + 0x35] = (0x36 + 8 * count) as u8;
    bytes[base + 0x47] = (3 + 4 * count) as u8;
    i = base + 0x4a;
    bytes[i] = 1;
    i += 4;
    bytes[i] = 2;
    i += 4;
    bytes[i] = 3;
    i += 4;
    bytes[i] = 0x38;
    i += 3;
    let mut current = ((count + 1) * 16) as u8;
    bytes[i] = current;
    i += 1;
    bytes[i] = 3;
    i += 4;
    bytes[i] = 0x38;
    i -= 5;
    for (track, entry) in text.iter().take(2).enumerate() {
        if let Some(value) = entry {
            current = current.wrapping_add((value.len() + 1) as u8);
        }
        if (track as isize) < count as isize - 2 {
            i += 8;
            bytes[i + 1] = 3;
            bytes[i + 5] = 0x38;
        } else if track as isize == count as isize - 2 {
            i += 8;
            bytes[i + 1] = 2;
            bytes[i + 5] = 3;
            bytes[i + 9] = 0x38;
        } else {
            i += 12;
            bytes[i + 1] = 3;
            bytes[i + 5] = 0x38;
        }
        bytes[i] = current;
    }
    for (track, entry) in text.iter().take(count - 1).enumerate() {
        i += 8;
        if let Some(value) = entry {
            current = current.wrapping_add((value.len() + 1) as u8);
        }
        bytes[i] = current;
        if track < count - 2 {
            bytes[i + 1] = 3;
            bytes[i + 5] = 0x38;
        }
    }
    i += 1;
    for entry in text.iter().take(count) {
        if let Some(value) = entry {
            let end = i.checked_add(value.len()).ok_or("Text table overflow")?;
            if end >= bytes.len() {
                return Err("Text exceeds allocated AMG table".into());
            }
            bytes[i..end].copy_from_slice(value.as_bytes());
            i = end;
        }
        bytes[i] = 9;
        i += 1;
    }
    bytes[base + 0x13] = (i - 1) as u8;
    bytes[base + 0x1f] = (i - 1 - 0x1c) as u8;
    Ok(())
}
