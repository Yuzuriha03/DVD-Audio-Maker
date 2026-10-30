#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParameterBlock {
    RestartInitial,
    RestartMain,
    Continuation,
}

#[derive(Default)]
pub(crate) struct Bits {
    pub(crate) words: Vec<u32>,
    pub(crate) pending: u32,
    pub(crate) width: u32,
    count_only: bool,
    counted_words: usize,
}
impl Bits {
    pub(crate) fn counting() -> Self {
        Self {
            count_only: true,
            ..Self::default()
        }
    }

    pub(crate) fn copy_mode_from(&mut self, other: &Self) {
        self.count_only = other.count_only;
    }

    pub(crate) fn has_counted_words(&self) -> bool {
        self.counted_words != 0
    }

    pub(crate) fn fork(&self) -> Self {
        Self {
            pending: self.pending,
            width: self.width,
            count_only: self.count_only,
            ..Self::default()
        }
    }

    pub(crate) fn commit(&mut self, local: Self) {
        self.counted_words += local.counted_words;
        self.words.extend(local.words);
        self.pending = local.pending;
        self.width = local.width;
    }

    pub(crate) fn word_len(&self) -> usize {
        self.words.len() + self.counted_words
    }

    pub(crate) fn finish_into(self, writer: &mut Self, last: bool) {
        if self.count_only {
            writer.counted_words =
                self.word_len() + usize::from(self.width != 0) + if last { 3 } else { 1 };
        } else {
            writer.words = self.seal(last);
        }
    }

    #[inline(always)]
    pub(crate) fn put(&mut self, value: u32, width: u32) {
        debug_assert!(width <= 32 && (width == 32 || value >> width == 0));

        if self.count_only {
            let bits = self.width + width;
            self.counted_words += (bits / 16) as usize;
            self.width = bits % 16;
            return;
        }
        let packed = (u64::from(self.pending) << width) | u64::from(value);
        let bits = self.width + width;
        if bits >= 16 {
            self.words.push(((packed >> (bits - 16)) & 0xffff) as u32);
            if bits >= 32 {
                self.words.push(((packed >> (bits - 32)) & 0xffff) as u32);
            }
        }
        self.width = bits % 16;
        self.pending = (packed & ((1u64 << self.width) - 1)) as u32;
    }
    pub(crate) fn seal(mut self, last: bool) -> Vec<u32> {
        if self.width != 0 {
            self.words.push(self.pending << (16 - self.width));
        }
        if last {
            self.words.extend([0xd234, 0xd234]);
        }
        self.words.push(checksum(&self.words));
        self.words
    }
}
pub(crate) fn checksum(words: &[u32]) -> u32 {
    let (mut x, mut c) = (0xa9u32, 0xa2u32);

    for &w in words {
        let t = ((c as i32) >> 7) as u32;
        x ^= w;
        c = ((((c & 0x7f) << 12) ^ t) << 4) ^ t ^ w;
    }

    for _ in 0..15 {
        if c & 0x400000 != 0 {
            c ^= 0x58c000;
        }
        c <<= 1;
    }

    ((x & 255) << 8) ^ (x & 0xff00) | ((c as i32 >> 15) as u32)
}
