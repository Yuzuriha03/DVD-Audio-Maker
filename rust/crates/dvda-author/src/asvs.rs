//! Still video information, ported from `asvs.c` with checked picture offsets.
use crate::atsi::Title;

#[derive(Clone, Debug)]
pub struct Options {
    pub video_attribute: u8,
    /// Background, text, highlight, selected foreground (packed Y/Cr/Cb).
    pub palette: [u32; 4],
    pub sectors: Option<u8>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            video_attribute: 0x53,
            palette: [0x108080, 0xeb8080, 0x108080, 0x808080],
            sectors: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub sectors: u8,
    pub vob_sectors: u32,
    pub picture_titles: u16,
}

fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_be_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_be_bytes());
}

/// The picture sizes follow group/title/track/picture order and refer to each
/// complete still VOB segment. Titles without new pictures reuse previous ASVS
/// entries and are omitted here, matching ATSI's picture-title rank.
pub fn encode(
    groups: &[Vec<Title>],
    picture_sectors: &[u32],
    options: &Options,
) -> Result<Encoded, String> {
    if groups.is_empty() || groups.len() > 9 {
        return Err("ASVS requires 1..=9 audio groups".into());
    }
    let titles: Vec<usize> = groups
        .iter()
        .flatten()
        .map(|t| t.tracks.iter().map(|t| t.pictures.len()).sum::<usize>())
        .filter(|&n| n > 0)
        .collect();
    if titles.is_empty() || titles.len() > 99 || titles.iter().any(|&n| n > 255) {
        return Err(
            "ASVS fixed directory supports 1..=99 picture titles and 1..=255 pictures per title"
                .into(),
        );
    }
    let count: usize = titles.iter().sum();
    if count != picture_sectors.len() || picture_sectors.contains(&0) {
        return Err("ASVS needs one nonzero VOB sector size per picture".into());
    }
    let required = (0x378 + 2 * count).div_ceil(2048).max(2);
    let sectors = options
        .sectors
        .unwrap_or(u8::try_from(required).map_err(|_| "ASVS exceeds 255 sectors")?);
    if usize::from(sectors) < required {
        return Err("ASVS sector capacity is too small".into());
    }
    let mut bytes = vec![0; usize::from(sectors) * 2048];
    bytes[..12].copy_from_slice(b"DVDAUDIOASVS");
    put16(&mut bytes, 0xc, titles.len() as u16);
    put16(&mut bytes, 0xe, 0x12);
    bytes[0x13] = 2;
    bytes[0x18] = options.video_attribute;
    for (offset, value) in [0x20, 0x28, 0x2c, 0x30].into_iter().zip(options.palette) {
        put32(&mut bytes, offset, value);
    }
    let mut index = 0;
    let mut total = 0u32;
    for (rank, &pictures) in titles.iter().enumerate() {
        let record = 0x60 + 8 * rank;
        bytes[record] = pictures as u8;
        put16(
            &mut bytes,
            record + 2,
            u16::try_from(index + 1).map_err(|_| "ASVS global picture rank exceeds 16 bits")?,
        );
        put32(&mut bytes, record + 4, total);
        let mut relative = 0u32;
        for &size in &picture_sectors[index..index + pictures] {
            put16(
                &mut bytes,
                0x378 + 2 * index,
                u16::try_from(relative)
                    .map_err(|_| "ASVS title relative picture offset exceeds 16 bits")?,
            );
            relative = relative
                .checked_add(size)
                .ok_or("ASVS title sector length exceeds 32 bits")?;
            total = total
                .checked_add(size)
                .ok_or("ASVS total VOB size exceeds 32 bits")?;
            index += 1;
        }
    }
    put32(&mut bytes, 0x14, total - 1);
    Ok(Encoded {
        bytes,
        sectors,
        vob_sectors: total,
        picture_titles: titles.len() as u16,
    })
}
