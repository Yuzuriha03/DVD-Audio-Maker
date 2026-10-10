pub const SPEAKERS: [&[u16]; 21] = [
    &[4],
    &[1, 2],
    &[1, 2, 256],
    &[1, 2, 16, 32],
    &[1, 2, 8],
    &[1, 2, 8, 256],
    &[1, 2, 8, 16, 32],
    &[1, 2, 4],
    &[1, 2, 4, 256],
    &[1, 2, 4, 16, 32],
    &[1, 2, 4, 8],
    &[1, 2, 4, 8, 256],
    &[1, 2, 4, 8, 16, 32],
    &[1, 2, 4, 256],
    &[1, 2, 4, 16, 32],
    &[1, 2, 4, 8],
    &[1, 2, 4, 8, 256],
    &[1, 2, 4, 8, 16, 32],
    &[1, 2, 16, 32, 8],
    &[1, 2, 16, 32, 4],
    &[1, 2, 16, 32, 4, 8],
];
pub const FIRST: [usize; 21] = [
    1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4,
];
pub const MEANING: [u32; 21] = [
    31, 27, 31, 25, 3, 31, 1, 26, 31, 24, 2, 31, 0, 31, 24, 2, 31, 0, 1, 24, 0,
];
#[derive(Clone, Debug)]
pub struct Profile {
    pub assignment: usize,
    pub group2_bits: u8,
    pub group2_rate: u32,
}
impl Profile {
    pub fn standard(c: &crate::Config) -> Self {
        Self {
            assignment: [0, 1, 7, 3, 9, 12]
                .get(c.channels.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(21),
            group2_bits: c.bits,
            group2_rate: c.sample_rate,
        }
    }
    pub fn valid(&self, c: &crate::Config) -> bool {
        self.assignment < 21
            && SPEAKERS[self.assignment].len() == c.channels as usize
            && (FIRST[self.assignment] == c.channels as usize
                || ([16, 20, 24].contains(&self.group2_bits)
                    && self.group2_bits <= c.bits
                    && (self.group2_rate == c.sample_rate
                        || ([88200, 96000].contains(&c.sample_rate)
                            && self.group2_rate == c.sample_rate / 2))))
    }
    pub fn order(&self) -> Vec<usize> {
        let speakers = SPEAKERS[self.assignment];
        speakers
            .iter()
            .map(|s| speakers.iter().filter(|t| *t < s).count())
            .collect()
    }
    pub fn depth(&self, c: &crate::Config, ch: usize) -> u8 {
        if ch < FIRST[self.assignment] {
            c.bits
        } else {
            self.group2_bits
        }
    }
}
