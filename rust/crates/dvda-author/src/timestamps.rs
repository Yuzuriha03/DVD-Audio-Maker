/// Encode the MPEG-2 system clock reference without rounding.
pub fn pack_scr(base: u64, extension: u16) -> [u8; 6] {
    let lsb = base & 0xffff_ffff;
    [
        (0x44 | ((base >> 32 & 1) << 5) | ((lsb >> 27) & 0x18) | ((lsb >> 28) & 3)) as u8,
        (lsb >> 20) as u8,
        (((lsb & 0x000f_8000) >> 12) | 4 | ((lsb & 0x6000) >> 13)) as u8,
        (lsb >> 5) as u8,
        (((lsb & 31) << 3) | 4 | u64::from((extension & 0x180) >> 7)) as u8,
        (((extension & 0x7f) << 1) | 1) as u8,
    ]
}

/// Encode the original author's 32-bit PTS/DTS domain.
pub fn pack_timestamp(value: u32, prefix: u8) -> [u8; 5] {
    [
        prefix | (((value >> 30) as u8 & 3) << 1) | 1,
        (value >> 22) as u8,
        (((value >> 15) as u8) << 1) | 1,
        (value >> 7) as u8,
        ((value as u8) << 1) | 1,
    ]
}

pub fn pack_header(clock: u64) -> [u8; 14] {
    let mut bytes = [0, 0, 1, 0xba, 0, 0, 0, 0, 0, 0, 1, 0x89, 0xc3, 0xf8];
    bytes[4..10].copy_from_slice(&pack_scr(clock / 300, (clock % 300) as u16));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_clock_matches_author() {
        assert_eq!(pack_scr(0, 0), [0x44, 0, 4, 0, 4, 1]);
        assert_eq!(pack_timestamp(0, 0x20), [0x21, 0, 1, 0, 1]);
        assert_eq!(
            pack_timestamp(u32::MAX, 0x30),
            [0x37, 0xff, 0xff, 0xff, 0xff]
        );
    }
}
