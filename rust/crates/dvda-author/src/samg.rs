#[derive(Clone, Copy, Debug)]
pub struct Track {
    pub mlp: bool,
    pub channels: u8,
    pub bits: u8,
    pub rate: u32,
    pub channel_assignment: u8,
    pub first_pts: u32,
    pub pts_length: u32,
    pub first_sector: u32,
    pub last_sector: u32,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub start_sector: u32,
    pub samg_sectors: u8,
    pub amg_sectors: u8,
    pub asvs_sectors: u8,
    pub atsi_sectors: Vec<u8>,
    pub still_vob_sectors: u32,
    pub top_vob_sectors: u32,
    pub video_link_tracks: u16,
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

/// Produce all eight copies of AUDIO_PP.IFO in original group/track order.
pub fn encode(groups: &[Vec<Track>], layout: &Layout) -> Result<(Vec<u8>, u32), String> {
    if groups.is_empty() || groups.len() > 9 || layout.atsi_sectors.len() < groups.len() {
        return Err("SAMG requires 1–9 groups and each ATSI size".into());
    }
    let size = usize::from(layout.samg_sectors) * 2048;
    if size == 0 || !size.is_multiple_of(8) {
        return Err("Invalid SAMG sector count".into());
    }
    let matrix_size = size / 8;
    let count = groups.iter().map(Vec::len).sum::<usize>();
    if count > (matrix_size.saturating_sub(16)) / 52
        || count + usize::from(layout.video_link_tracks) > usize::from(u16::MAX)
    {
        return Err("SAMG track table exceeds matrix capacity".into());
    }
    let mut bytes = vec![0; size];
    bytes[..12].copy_from_slice(b"DVDAUDIOSAPP");
    bytes[12..14]
        .copy_from_slice(&((count + usize::from(layout.video_link_tracks)) as u16).to_be_bytes());
    bytes[14..16].copy_from_slice(&0x12_u16.to_be_bytes());
    let mut absolute = layout
        .start_sector
        .wrapping_add(u32::from(layout.samg_sectors))
        .wrapping_add(2 * (u32::from(layout.amg_sectors) + u32::from(layout.asvs_sectors)))
        .wrapping_add(layout.top_vob_sectors)
        .wrapping_add(layout.still_vob_sectors)
        .wrapping_add(u32::from(layout.atsi_sectors[0]));
    let mut last = 0;
    let mut position = 16;
    for (group_index, tracks) in groups.iter().enumerate() {
        if tracks.is_empty() || tracks.len() > 255 {
            return Err("SAMG groups require 1–255 tracks".into());
        }
        for (track_index, track) in tracks.iter().enumerate() {
            let bits = match track.bits {
                16 => 0,
                20 => 1,
                24 => 2,
                _ => return Err("Unsupported sample width".into()),
            };
            let rate = match track.rate {
                48000 => 0,
                96000 => 1,
                192000 => 2,
                44100 => 8,
                88200 => 9,
                176400 => 10,
                _ => return Err("Unsupported sample rate".into()),
            };
            bytes[position + 2] = (group_index + 1) as u8;
            bytes[position + 3] = (track_index + 1) as u8;
            put32(&mut bytes, position + 4, track.first_pts);
            put32(&mut bytes, position + 8, track.pts_length);
            bytes[position + 16] = match (track_index == 0, track.mlp, track.channels > 2) {
                (true, true, _) => 0xd8,
                (false, true, _) => 0x58,
                (true, false, true) => 0xc0,
                (true, false, false) => 0xc8,
                (false, false, _) => 0x40,
            };
            bytes[position + 17] = if track.channels > 2 {
                bits * 0x11
            } else {
                bits * 0x10 + 0xf
            };
            bytes[position + 18] = if track.channels > 2 {
                rate * 0x11
            } else {
                rate * 0x10 + 0xf
            };
            bytes[position + 19] = track.channel_assignment;
            put32(
                &mut bytes,
                position + 40,
                absolute.wrapping_add(track.first_sector),
            );
            put32(
                &mut bytes,
                position + 44,
                absolute.wrapping_add(track.first_sector),
            );
            last = absolute.wrapping_add(track.last_sector);
            put32(&mut bytes, position + 48, last);
            position += 52;
        }
        if group_index + 1 < groups.len() {
            absolute = absolute
                .wrapping_add(tracks[tracks.len() - 1].last_sector)
                .wrapping_add(1)
                .wrapping_add(u32::from(layout.atsi_sectors[group_index]))
                .wrapping_add(u32::from(layout.atsi_sectors[group_index + 1]));
        }
    }
    for copy in 1..8 {
        bytes.copy_within(0..matrix_size, copy * matrix_size);
    }
    Ok((bytes, last))
}
