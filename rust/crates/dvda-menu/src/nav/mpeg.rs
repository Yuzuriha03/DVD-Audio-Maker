//! MPEG program-stream primitives needed by the navigation VOB writer.
//!
//! These are the small, testable pieces of the C VOB builder: SCR/PTS field
//! codecs and timestamp transposition. They operate on borrowed sectors instead
//! of touching global state.

const PACK: u8 = 0xba;
const SYSTEM: u8 = 0xbb;
const PRIVATE_1: u8 = 0xbd;
const AUDIO_FIRST: u8 = 0xc0;
const VIDEO_LAST: u8 = 0xef;

fn be16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

/// Reads the 90 kHz SCR from a PACK header body (the four bytes after the start code).
pub(crate) fn read_scr(bytes: &[u8]) -> u32 {
    (u32::from(bytes[0] & 0x38) << 27)
        | (u32::from(bytes[0] & 3) << 28)
        | (u32::from(bytes[1]) << 20)
        | (u32::from(bytes[2] & 0xf8) << 12)
        | (u32::from(bytes[2] & 3) << 13)
        | (u32::from(bytes[3]) << 5)
        | (u32::from(bytes[4] & 0xf8) >> 3)
}

/// Writes the 90 kHz SCR into a PACK header body, preserving the 27 MHz extension bits.
#[expect(
    clippy::cast_possible_truncation,
    reason = "SCR header bytes intentionally retain the low eight bits of masked fields"
)]
pub(crate) fn write_scr(bytes: &mut [u8], scr: u32) {
    bytes[0] = (((scr >> 27) & 0x38) | ((scr >> 28) & 3)) as u8 | 0x44;
    bytes[1] = (scr >> 20) as u8;
    bytes[2] = ((scr >> 12) & 0xf8) as u8 | ((scr >> 13) & 3) as u8 | 4;
    bytes[3] = (scr >> 5) as u8;
    bytes[4] = ((scr << 3) & 0xf8) as u8 | (bytes[4] & 7);
}

/// Reads a 90 kHz PTS/DTS from a PES header.
pub(crate) fn read_pts(bytes: &[u8]) -> u32 {
    let a1 = u32::from((bytes[0] & 0x0e) >> 1);
    let a2 = (u32::from(bytes[1]) << 8 | u32::from(bytes[2])) >> 1;
    let a3 = (u32::from(bytes[3]) << 8 | u32::from(bytes[4])) >> 1;
    (a1 << 30) | (a2 << 15) | a3
}

/// Writes a 90 kHz PTS/DTS into a PES header, preserving the marker/top bits.
#[expect(
    clippy::cast_possible_truncation,
    reason = "PTS header words and bytes intentionally retain low protocol bits"
)]
pub(crate) fn write_pts(bytes: &mut [u8], pts: u32) {
    bytes[0] = ((pts >> 29) & 0x0e) as u8 | (bytes[0] & 0xf1);
    let word1 = ((pts >> 14) | 1) as u16;
    bytes[1] = (word1 >> 8) as u8;
    bytes[2] = word1 as u8;
    let word2 = ((pts << 1) | 1) as u16;
    bytes[3] = (word2 >> 8) as u8;
    bytes[4] = word2 as u8;
}

fn sysoffs(sector: &[u8]) -> usize {
    if sector[14] == 0 && sector[15] == 0 && sector[16] == 1 && sector[17] == SYSTEM {
        usize::from(be16(&sector[18..])) + 6
    } else {
        0
    }
}

/// Adjusts the PACK SCR and the PES PTS/DTS by the given 90 kHz offset.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Timestamp transposition intentionally wraps to the 32-bit MPEG clock"
)]
pub(crate) fn transpose_ts(sector: &mut [u8], offset: i64) {
    if sector[0] != 0 || sector[1] != 0 || sector[2] != 1 || sector[3] != PACK {
        return;
    }
    let sys = sysoffs(sector);
    let scr = i64::from(read_scr(&sector[4..9])) + offset;
    write_scr(&mut sector[4..9], scr as u32);
    let at = 14 + sys;
    let stream = sector[at + 3];
    if sector[at] != 0 || sector[at + 1] != 0 || sector[at + 2] != 1 {
        return;
    }
    if !(stream == PRIVATE_1 || (AUDIO_FIRST..=VIDEO_LAST).contains(&stream)) {
        return;
    }
    if sector[at + 7] & 128 == 0 {
        return;
    }
    let pts = i64::from(read_pts(&sector[at + 9..])) + offset;
    write_pts(&mut sector[at + 9..], pts as u32);
    if sector[at + 7] & 64 != 0 {
        let dts = i64::from(read_pts(&sector[at + 14..])) + offset;
        write_pts(&mut sector[at + 14..], dts as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scr_roundtrip_covers_all_bits() {
        for value in [0u32, 1, 0x3fff_ffff, 0x1555_5555] {
            let mut bytes = [0u8; 5];
            write_scr(&mut bytes, value);
            assert_eq!(read_scr(&bytes), value);
        }
    }

    #[test]
    fn pts_roundtrip_covers_all_bits() {
        for value in [0u32, 1, 0x7fff_ffff, 0x2aaa_aaaa] {
            let mut bytes = [0x21u8, 0, 0, 0, 0];
            write_pts(&mut bytes, value);
            assert_eq!(read_pts(&bytes), value);
        }
    }

    fn sample_sector() -> [u8; 2048] {
        let mut sector = [0u8; 2048];
        sector[0] = 0;
        sector[1] = 0;
        sector[2] = 1;
        sector[3] = PACK;
        sector[4] = 0x44;
        sector[5] = 0x00;
        sector[6] = 0x04;
        sector[7] = 0x01;
        sector[8] = 0x23;
        sector[14] = 0;
        sector[15] = 0;
        sector[16] = 1;
        sector[17] = 0xe0;
        sector[20] = 0x80;
        sector[21] = 0xc0;
        write_pts(&mut sector[23..28], 90_000);
        // The first PES has PTS and DTS; keep DTS well-formed for transposition.
        write_pts(&mut sector[28..33], 90_000);
        sector
    }

    #[test]
    fn transpose_is_reversible_on_a_sample_pack() {
        let original = sample_sector();
        let mut sector = original;
        transpose_ts(&mut sector, 12_345);
        assert_ne!(sector, original);
        transpose_ts(&mut sector, -12_345);
        assert_eq!(sector, original);
    }
}
