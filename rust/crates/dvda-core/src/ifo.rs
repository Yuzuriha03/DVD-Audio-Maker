//! Bounded ATS program-chain parsing used by the authored-disc audit.
use crate::verification::TrackRow;

#[derive(Debug, Default)]
pub struct Group {
    pub rows: Vec<TrackRow>,
    pub maximum_still: u8,
    pub issues: Vec<String>,
}
fn u16_at(data: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_be_bytes(
        data.get(offset..offset + 2)
            .ok_or("IFO_TRUNCATED")?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        data.get(offset..offset + 4)
            .ok_or("IFO_TRUNCATED")?
            .try_into()
            .unwrap(),
    ))
}
pub fn parse(data: &[u8], group: i32) -> Result<Group, String> {
    if data.len() < 212 {
        return Err("IFO_SHORT".into());
    }
    if !data.starts_with(b"DVDAUDIO-ATS") {
        return Err("IFO_SIGNATURE_INVALID".into());
    }
    let pointer = (u32_at(data, 204)? as usize)
        .checked_mul(2048)
        .ok_or("PGC_OUT_OF_RANGE")?;
    let remaining = data.get(pointer..).ok_or("PGC_OUT_OF_RANGE")?;
    let length = (u32_at(remaining, 4)? as usize)
        .checked_add(1)
        .ok_or("PGC_OUT_OF_RANGE")?;
    let pgc = remaining
        .get(..length)
        .filter(|p| p.len() >= 8)
        .ok_or("PGC_OUT_OF_RANGE")?;
    let titles = u16_at(pgc, 0)? as usize;
    if titles == 0 || 8 + titles * 8 > pgc.len() {
        return Err("TITLE_OUT_OF_RANGE".into());
    }
    let mut result = Group::default();
    for title in 0..titles {
        let offset = u32_at(pgc, 12 + title * 8)? as usize;
        let header = pgc.get(offset..offset + 16).ok_or("TITLE_OUT_OF_RANGE")?;
        let tracks = header[2] as usize;
        if tracks == 0 {
            return Err("IFO_TRACKS_MISSING".into());
        }
        let title_length = u32_at(header, 4)? as i64;
        let sectors = offset + u16_at(header, 12)? as usize;
        let pictures = u16_at(header, 14)? as usize;
        let mut previous = None;
        for track in 0..tracks {
            let cell = offset + 16 + track * 20;
            let first_pts = u32_at(pgc, cell + 6)? as i64;
            let length = u32_at(pgc, cell + 10)? as i64;
            if previous.is_some_and(|v| first_pts <= v) {
                result.issues.push("PGC_TIMELINE_NOT_INCREASING".into());
            }
            previous = Some(first_pts);
            if tracks > 1
                && track + 1 == tracks
                && (first_pts + length - title_length).abs() > 90000
            {
                result.issues.push("PGC_TIMELINE_LENGTH_MISMATCH".into());
            }
            let first = i32::try_from(u32_at(pgc, sectors + track * 12 + 4)?)
                .map_err(|_| "CELL_OUT_OF_RANGE")?;
            let last = i32::try_from(u32_at(pgc, sectors + track * 12 + 8)?)
                .map_err(|_| "CELL_OUT_OF_RANGE")?;
            if last < first {
                result.issues.push("CELL_OUT_OF_RANGE".into());
            }
            result.rows.push(TrackRow {
                group,
                title: title as i32 + 1,
                track: track as i32 + 1,
                first,
                last,
                pts: first_pts,
                length,
            });
            if pictures != 0 {
                let picture = *pgc
                    .get(offset + pictures + track * 6)
                    .ok_or("STILL_REFERENCE_OUT_OF_RANGE")?;
                result.maximum_still = result.maximum_still.max(picture);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut data = vec![0; 4096];
        data[..12].copy_from_slice(b"DVDAUDIO-ATS");
        data[204..208].copy_from_slice(&1u32.to_be_bytes());
        let pgc = &mut data[2048..];
        pgc[..2].copy_from_slice(&1u16.to_be_bytes());
        pgc[4..8].copy_from_slice(&127u32.to_be_bytes());
        pgc[12..16].copy_from_slice(&16u32.to_be_bytes());
        pgc[18] = 2;
        pgc[20..24].copy_from_slice(&2000u32.to_be_bytes());
        pgc[28..30].copy_from_slice(&56u16.to_be_bytes());
        pgc[42..46].copy_from_slice(&1000u32.to_be_bytes());
        pgc[58..62].copy_from_slice(&1000u32.to_be_bytes());
        pgc[62..66].copy_from_slice(&1000u32.to_be_bytes());
        pgc[80..84].copy_from_slice(&4u32.to_be_bytes());
        pgc[88..92].copy_from_slice(&5u32.to_be_bytes());
        pgc[92..96].copy_from_slice(&9u32.to_be_bytes());
        data
    }
    #[test]
    fn reads_actual_track_boundaries_and_rejects_truncated_tables() {
        let data = fixture();
        let parsed = parse(&data, 1).unwrap();
        assert!(parsed.issues.is_empty());
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!((parsed.rows[1].first, parsed.rows[1].last), (5, 9));
        for length in [0, 100, 212, 2048, 2100] {
            assert!(parse(&data[..length], 1).is_err());
        }
        let mut bad = data.clone();
        bad[2060..2064].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse(&bad, 1).is_err());
        let mut bad = data;
        bad[2106..2110].fill(0);
        assert!(
            parse(&bad, 1)
                .unwrap()
                .issues
                .contains(&"PGC_TIMELINE_NOT_INCREASING".into())
        );
    }
}
