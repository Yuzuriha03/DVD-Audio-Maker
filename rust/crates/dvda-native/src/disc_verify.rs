//! Streaming, read-only DVD-Audio PES payload verification.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct ResultInfo {
    pub code: i32,
    pub track: i32,
    pub offset: u64,
    pub sectors: u64,
    pub bytes: u64,
}

pub struct NativeDiscVerifier;

impl NativeDiscVerifier {
    pub fn load() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn verify_mlp<I>(&self, sources: &[PathBuf], chunks: I) -> Result<ResultInfo, String>
    where
        I: Iterator<Item = Result<Vec<u8>, String>>,
    {
        self.verify(sources, None, chunks)
    }

    pub fn verify_lpcm<I>(
        &self,
        sources: &[PathBuf],
        title_ends: &[u8],
        chunks: I,
    ) -> Result<ResultInfo, String>
    where
        I: Iterator<Item = Result<Vec<u8>, String>>,
    {
        if title_ends.len() != sources.len() {
            return Err("LPCM title boundary count does not match source count".into());
        }
        self.verify(sources, Some(title_ends), chunks)
    }

    fn verify<I>(
        &self,
        sources: &[PathBuf],
        ends: Option<&[u8]>,
        mut chunks: I,
    ) -> Result<ResultInfo, String>
    where
        I: Iterator<Item = Result<Vec<u8>, String>>,
    {
        if sources.is_empty() {
            return Err("At least one source track is required".into());
        }
        for path in sources {
            if path.to_string_lossy().contains('\0') {
                return Err(format!("Source path contains NUL: {}", path.display()));
            }
        }
        let mut compare = Compare::new(sources);
        let mut pcm = ends.map(Pcm::new);
        let mut status = if let Some(p) = &mut pcm {
            if !p.ends.last().is_some_and(|end| *end != 0) {
                -9
            } else {
                p.open(&mut compare).err().unwrap_or(0)
            }
        } else {
            0
        };
        while status == 0 {
            let next =
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| chunks.next())) {
                    Ok(next) => next,
                    Err(_) => return Err("ISO reader callback panicked".into()),
                };
            let chunk = match next {
                None => break,
                Some(Err(error)) => return Err(error),
                Some(Ok(chunk)) => chunk,
            };
            if chunk.is_empty() {
                continue;
            }
            if chunk.len() % 2048 != 0 {
                return Err("ISO verifier chunk is not sector aligned".into());
            }
            for block in chunk.chunks(128 * 1024) {
                for sector in block.as_chunks::<2048>().0 {
                    let checked = if let Some(p) = &mut pcm {
                        p.sector(&mut compare, sector)
                    } else {
                        compare.sector(sector)
                    };
                    if let Err(code) = checked {
                        status = code;
                        break;
                    }
                }
                if status != 0 {
                    break;
                }
            }
        }
        if status == 0 {
            if let Some(p) = &pcm {
                if compare.file.is_some() || compare.index != sources.len() || p.at != p.length {
                    status = -7;
                }
            } else if compare.file.is_some() || compare.index != sources.len() {
                status = -7;
                compare.result.track = compare.index as i32;
                compare.result.offset = compare.offset;
            }
        }
        compare.result.code = status;
        if status != 0 {
            return Err(format!(
                "DVD audio payload verification failed: status {}, track {}, offset {}, sectors {}",
                status,
                compare.result.track + 1,
                compare.result.offset,
                compare.result.sectors
            ));
        }
        Ok(compare.result)
    }
}

struct Compare<'a> {
    sources: &'a [PathBuf],
    index: usize,
    file: Option<File>,
    left: u64,
    offset: u64,
    result: ResultInfo,
}
impl<'a> Compare<'a> {
    fn new(sources: &'a [PathBuf]) -> Self {
        Self {
            sources,
            index: 0,
            file: None,
            left: 0,
            offset: 0,
            result: ResultInfo::default(),
        }
    }
    fn consume(&mut self, mut data: &[u8]) -> Result<(), i32> {
        while !data.is_empty() {
            self.result.track = self.index as i32;
            self.result.offset = self.offset;
            if self.file.is_none() {
                if self.index == self.sources.len() {
                    return Err(-6);
                }
                let file = File::open(&self.sources[self.index]).map_err(|_| -4)?;
                let size = file.metadata().map_err(|_| -4)?.len();
                if size == 0 {
                    return Err(-4);
                }
                self.file = Some(file);
                self.left = size;
                self.offset = 0;
            }
            let n = (data.len() as u64).min(self.left) as usize;
            let mut expected = vec![0; n];
            self.file
                .as_mut()
                .unwrap()
                .read_exact(&mut expected)
                .map_err(|_| -4)?;
            if let Some(i) = data[..n].iter().zip(&expected).position(|(a, b)| a != b) {
                self.result.offset = self.offset + i as u64;
                return Err(-5);
            }
            self.offset += n as u64;
            self.left -= n as u64;
            self.result.bytes += n as u64;
            data = &data[n..];
            if self.left == 0 {
                self.file = None;
                self.index += 1;
                self.offset = 0;
            }
        }
        Ok(())
    }
    fn sector(&mut self, s: &[u8]) -> Result<(), i32> {
        packets(s, |id, _, end, audio| {
            if id == 0xbd {
                let q = audio.ok_or(-2)?;
                if q + 4 > end {
                    return Err(-2);
                }
                if s[q] != 0xa1 {
                    return Err(-3);
                }
                if s[q + 3] < 6 {
                    return Err(-2);
                }
                let start = q + 4 + s[q + 3] as usize;
                if start > end {
                    return Err(-2);
                }
                self.consume(&s[start..end])
            } else {
                Ok(())
            }
        })?;
        self.result.sectors += 1;
        Ok(())
    }
}

fn be16(s: &[u8]) -> usize {
    u16::from_be_bytes([s[0], s[1]]) as usize
}
fn le16(s: &[u8]) -> usize {
    u16::from_le_bytes([s[0], s[1]]) as usize
}
fn le32(s: &[u8]) -> usize {
    u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize
}

// The callback receives the private-stream header offset and packet end.
fn packets(
    s: &[u8],
    mut packet: impl FnMut(u8, usize, usize, Option<usize>) -> Result<(), i32>,
) -> Result<(), i32> {
    if s.len() != 2048 || s[..4] != [0, 0, 1, 0xba] || s[4] & 0xc0 != 0x40 {
        return Err(-1);
    }
    let mut at = 14 + (s[13] & 7) as usize;
    while at < 2048 {
        if s[at..].starts_with(&[0, 0, 1, 0xb9]) {
            if s[at + 4..].iter().any(|b| *b != 0xff) {
                return Err(-2);
            }
            break;
        }
        if s[at..].iter().all(|b| *b == 0) {
            break;
        }
        if at + 6 > 2048 || s[at..at + 3] != [0, 0, 1] {
            return Err(-2);
        }
        let end = at + 6 + be16(&s[at + 4..]);
        let id = s[at + 3];
        if end > 2048 || (end == at + 6 && id != 0xbe) {
            return Err(-2);
        }
        let q = if id == 0xbd {
            if at + 9 > end || s[at + 6] & 0xc0 != 0x80 {
                return Err(-2);
            }
            let q = at + 9 + s[at + 8] as usize;
            Some(q)
        } else if id == 0xbb || id == 0xbe {
            None
        } else {
            return Err(-3);
        };
        packet(id, at, end, q)?;
        at = end;
    }
    Ok(())
}

// PCM two-frame interleave order follows the GPL DVD-Audio author implementation
// in tools/dvda-author-mlp8/src/audio.c (interleave_*_sample_extended).
const ORDER: [[&[usize]; 6]; 2] = [
    [
        &[1, 0, 3, 2],
        &[1, 0, 3, 2, 5, 4, 7, 6],
        &[5, 4, 11, 10, 1, 0, 3, 2, 7, 6, 9, 8],
        &[5, 4, 7, 6, 13, 12, 15, 14, 1, 0, 3, 2, 9, 8, 11, 10],
        &[
            7, 6, 9, 8, 17, 16, 19, 18, 1, 0, 3, 2, 5, 4, 11, 10, 13, 12, 15, 14,
        ],
        &[
            7, 6, 9, 8, 11, 10, 19, 18, 21, 20, 23, 22, 1, 0, 3, 2, 5, 4, 13, 12, 15, 14, 17, 16,
        ],
    ],
    [
        &[2, 1, 5, 4, 0, 3],
        &[2, 1, 5, 4, 8, 7, 11, 10, 0, 3, 6, 9],
        &[8, 7, 17, 16, 6, 15, 2, 1, 5, 4, 11, 10, 14, 13, 0, 3, 9, 12],
        &[
            8, 7, 11, 10, 20, 19, 23, 22, 6, 9, 18, 21, 2, 1, 5, 4, 14, 13, 17, 16, 0, 3, 12, 15,
        ],
        &[
            11, 10, 14, 13, 26, 25, 29, 28, 9, 12, 24, 27, 2, 1, 5, 4, 8, 7, 17, 16, 20, 19, 23,
            22, 0, 3, 6, 15, 18, 21,
        ],
        &[
            8, 7, 11, 10, 26, 25, 29, 28, 6, 9, 24, 27, 2, 1, 5, 4, 14, 13, 17, 16, 20, 19, 23, 22,
            32, 31, 35, 34, 0, 3, 12, 15, 18, 21, 30, 33,
        ],
    ],
];
const MASKS: [usize; 13] = [
    4, 3, 0x103, 0x33, 0xb, 0x10b, 0x3b, 7, 0x107, 0x37, 0xf, 0x10f, 0x3f,
];
const DEFAULT_MASKS: [usize; 6] = [4, 3, 7, 0x33, 0x37, 0x3f];

struct Pcm<'a> {
    ends: &'a [u8],
    channels: usize,
    bits: usize,
    rate: usize,
    cga: usize,
    packed: [u8; 36],
    at: usize,
    length: usize,
}
impl<'a> Pcm<'a> {
    fn new(ends: &'a [u8]) -> Self {
        Self {
            ends,
            channels: 0,
            bits: 0,
            rate: 0,
            cga: 0,
            packed: [0; 36],
            at: 0,
            length: 0,
        }
    }
    fn open(&mut self, c: &mut Compare<'_>) -> Result<(), i32> {
        let mut f = File::open(&c.sources[c.index]).map_err(|_| -4)?;
        c.file = Some(f.try_clone().map_err(|_| -4)?);
        let mut h = [0u8; 40];
        f.read_exact(&mut h[..12]).map_err(|_| -4)?;
        if &h[..4] != b"RIFF" || &h[8..12] != b"WAVE" {
            return Err(-4);
        }
        let end = le32(&h[4..8]) as u64 + 8;
        if f.metadata().map_err(|_| -4)?.len() != end {
            return Err(-4);
        }
        let (mut channels, mut bits, mut rate, mut align, mut mask) = (0, 0, 0, 0, 0);
        let (mut data, mut size) = (0, 0);
        while f.stream_position().map_err(|_| -4)? + 8 <= end {
            f.read_exact(&mut h[..8]).map_err(|_| -4)?;
            let n = le32(&h[4..8]) as u64;
            let pos = f.stream_position().map_err(|_| -4)?;
            let next = pos + n + (n & 1);
            if next > end {
                return Err(-4);
            }
            if &h[..4] == b"fmt " {
                if bits != 0 || n < 16 {
                    return Err(-4);
                }
                f.read_exact(&mut h[..(n as usize).min(40)])
                    .map_err(|_| -4)?;
                let tag = le16(&h[..2]);
                channels = le16(&h[2..4]);
                rate = le32(&h[4..8]);
                align = le16(&h[12..14]);
                bits = le16(&h[14..16]);
                if tag == 0xfffe {
                    const GUID: [u8; 16] =
                        [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113];
                    if n < 40
                        || le16(&h[16..18]) < 22
                        || le16(&h[18..20]) != bits
                        || h[24..40] != GUID
                    {
                        return Err(-4);
                    }
                    mask = le32(&h[20..24]);
                } else if tag != 1 {
                    return Err(-4);
                }
            } else if &h[..4] == b"data" {
                if data != 0 {
                    return Err(-4);
                }
                data = pos;
                size = n;
            }
            f.seek(SeekFrom::Start(next)).map_err(|_| -4)?;
        }
        if !(1..=6).contains(&channels)
            || (bits != 16 && bits != 24)
            || align != channels * (bits / 8)
            || data == 0
            || size == 0
            || size % align as u64 != 0
            || rate * channels * bits > 9_600_000
            || ![44100, 48000, 88200, 96000, 176400, 192000].contains(&rate)
            || rate > 96000 && channels > 2
        {
            return Err(-4);
        }
        if mask == 0 {
            mask = DEFAULT_MASKS[channels - 1];
        }
        let cga = MASKS.iter().position(|m| *m == mask).ok_or(-4)?;
        if self.channels != 0
            && (channels != self.channels
                || bits != self.bits
                || rate != self.rate
                || cga != self.cga)
        {
            return Err(-8);
        }
        self.channels = channels;
        self.bits = bits;
        self.rate = rate;
        self.cga = cga;
        c.left = size;
        c.offset = 0;
        f.seek(SeekFrom::Start(data)).map_err(|_| -4)?;
        c.file = Some(f);
        Ok(())
    }
    fn pair(&mut self, c: &mut Compare<'_>) -> Result<(), i32> {
        let mut raw = [0u8; 36];
        let mut used = 0;
        let unit = self.channels * (self.bits / 8) * 2;
        while used < unit {
            if c.file.is_none() {
                if c.index == c.sources.len() {
                    return Err(-6);
                }
                self.open(c)?;
            }
            let n = ((unit - used) as u64).min(c.left) as usize;
            c.file
                .as_mut()
                .unwrap()
                .read_exact(&mut raw[used..used + n])
                .map_err(|_| -4)?;
            c.result.track = c.index as i32;
            c.result.offset = c.offset;
            c.left -= n as u64;
            c.offset += n as u64;
            used += n;
            if c.left == 0 {
                let ended = c.index;
                c.file = None;
                c.index += 1;
                c.offset = 0;
                if self.ends[ended] != 0 {
                    break;
                }
            }
        }
        if used != unit && used != unit / 2 {
            return Err(-8);
        }
        for (i, from) in ORDER[usize::from(self.bits == 24)][self.channels - 1]
            .iter()
            .enumerate()
        {
            self.packed[i] = raw[*from];
        }
        self.at = 0;
        self.length = unit;
        Ok(())
    }
    fn sector(&mut self, c: &mut Compare<'_>, s: &[u8]) -> Result<(), i32> {
        packets(s, |id, _, end, q| {
            if id == 0xbd {
                let q = q.ok_or(-2)?;
                if q + 12 > end || s[q] != 0xa0 || s[q + 3] < 8 {
                    return Err(-3);
                }
                let start = q + 4 + s[q + 3] as usize;
                if start > end {
                    return Err(-2);
                }
                let rate = match self.rate {
                    48000 => 0,
                    96000 => 1,
                    192000 => 2,
                    44100 => 8,
                    88200 => 9,
                    _ => 10,
                };
                let bit = if self.bits == 24 { 2 } else { 0 };
                if s[q + 7] != ((bit << 4) | if self.channels <= 2 { 15 } else { bit })
                    || s[q + 8] != ((rate << 4) | if self.channels <= 2 { 15 } else { rate })
                    || s[q + 10] as usize != self.cga
                {
                    return Err(-8);
                }
                for &byte in &s[start..end] {
                    if self.at == self.length {
                        self.pair(c)?;
                    }
                    if byte != self.packed[self.at] {
                        return Err(-5);
                    }
                    self.at += 1;
                    c.result.bytes += 1;
                }
            }
            Ok(())
        })?;
        c.result.sectors += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    pub(super) struct Fixture(pub(super) PathBuf);
    impl Fixture {
        pub(super) fn new(data: &[u8]) -> Self {
            let path = std::env::current_dir().unwrap().join(format!(
                ".disc-verify-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::write(&path, data).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_file(&self.0).unwrap();
        }
    }
    pub(super) fn mlp(payload: &[u8]) -> Vec<u8> {
        let mut s = vec![0; 2048];
        s[..4].copy_from_slice(&[0, 0, 1, 0xba]);
        s[4] = 0x40;
        s[14..18].copy_from_slice(&[0, 0, 1, 0xbd]);
        s[18..20].copy_from_slice(&((payload.len() + 13) as u16).to_be_bytes());
        s[20] = 0x80;
        s[22] = 0;
        s[23] = 0xa1;
        s[26] = 6;
        s[33..33 + payload.len()].copy_from_slice(payload);
        s
    }
    pub(super) fn wave(data: &[u8]) -> Vec<u8> {
        let mut wav = Vec::from(&b"RIFF"[..]);
        wav.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&48000u32.to_le_bytes());
        wav.extend_from_slice(&192000u32.to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(data);
        wav
    }
    pub(super) fn lpcm(payload: &[u8]) -> Vec<u8> {
        let mut s = mlp(payload);
        s[23] = 0xa0;
        s[26] = 8;
        s[18..20].copy_from_slice(&((payload.len() + 15) as u16).to_be_bytes());
        s[30] = 0x0f;
        s[31] = 0x0f;
        s[33] = 1;
        s[35..35 + payload.len()].copy_from_slice(payload);
        s
    }
    #[test]
    fn layout_and_mlp_streaming() {
        assert_eq!(std::mem::size_of::<ResultInfo>(), 32);
        assert_eq!(std::mem::offset_of!(ResultInfo, offset), 8);
        let first = Fixture::new(b"ABC");
        let second = Fixture::new(b"DEF");
        let sources = [first.0.clone(), second.0.clone()];
        let verifier = NativeDiscVerifier::load().unwrap();
        let result = verifier
            .verify_mlp(
                &sources,
                vec![Ok(Vec::new()), Ok(mlp(b"ABCDEF"))].into_iter(),
            )
            .unwrap();
        assert_eq!((result.bytes, result.sectors, result.track), (6, 1, 1));
        assert!(
            verifier
                .verify_mlp(&sources, [Ok(mlp(b"ABCDEX"))].into_iter())
                .unwrap_err()
                .contains("status -5, track 2, offset 2")
        );
        assert!(
            verifier
                .verify_mlp(&sources, [Ok(mlp(b"ABC"))].into_iter())
                .unwrap_err()
                .contains("status -7, track 2")
        );
        assert!(
            verifier
                .verify_mlp(&sources, [Ok(mlp(b"ABCDEFG"))].into_iter())
                .unwrap_err()
                .contains("status -6, track 3")
        );
    }
    #[test]
    fn lpcm_pairs_and_final_odd_frame() {
        let first = Fixture::new(&wave(&[1, 2, 3, 4, 5, 6, 7, 8]));
        let result = NativeDiscVerifier::load().unwrap().verify_lpcm(
            std::slice::from_ref(&first.0),
            &[1],
            [Ok(lpcm(&[2, 1, 4, 3, 6, 5, 8, 7]))].into_iter(),
        );
        assert_eq!(result.unwrap().bytes, 8);
        let odd = Fixture::new(&wave(&[1, 2, 3, 4]));
        assert_eq!(
            NativeDiscVerifier::load()
                .unwrap()
                .verify_lpcm(
                    std::slice::from_ref(&odd.0),
                    &[1],
                    [Ok(lpcm(&[2, 1, 4, 3, 0, 0, 0, 0]))].into_iter()
                )
                .unwrap()
                .bytes,
            8
        );
    }
    #[test]
    fn lpcm_cross_track_pairs_and_titles() {
        let first = Fixture::new(&wave(&[1, 2, 3, 4]));
        let second = Fixture::new(&wave(&[5, 6, 7, 8]));
        let verifier = NativeDiscVerifier::load().unwrap();
        let paths = [first.0.clone(), second.0.clone()];
        let joined = lpcm(&[2, 1, 4, 3, 6, 5, 8, 7]);
        assert_eq!(
            verifier
                .verify_lpcm(&paths, &[0, 1], [Ok(joined)].into_iter())
                .unwrap()
                .bytes,
            8
        );
        let separated = lpcm(&[2, 1, 4, 3, 0, 0, 0, 0, 6, 5, 8, 7, 0, 0, 0, 0]);
        assert_eq!(
            verifier
                .verify_lpcm(&paths, &[1, 1], [Ok(separated)].into_iter())
                .unwrap()
                .bytes,
            16
        );
        assert!(
            verifier
                .verify_lpcm(&paths, &[1, 0], std::iter::empty())
                .unwrap_err()
                .contains("status -9")
        );
        assert_eq!(
            verifier
                .verify_lpcm(&paths, &[1], std::iter::empty())
                .unwrap_err(),
            "LPCM title boundary count does not match source count"
        );
    }
    #[test]
    fn sector_padding_and_packet_bounds() {
        let source = Fixture::new(b"X");
        let paths = [source.0.clone()];
        let verifier = NativeDiscVerifier::load().unwrap();
        let mut valid = mlp(b"X");
        valid[34..].fill(0xff);
        valid[34..38].copy_from_slice(&[0, 0, 1, 0xb9]);
        assert_eq!(
            verifier
                .verify_mlp(&paths, [Ok(valid.clone())].into_iter())
                .unwrap()
                .sectors,
            1
        );
        valid[2047] = 0;
        assert!(
            verifier
                .verify_mlp(&paths, [Ok(valid)].into_iter())
                .unwrap_err()
                .contains("status -2")
        );
        let mut truncated = mlp(b"X");
        truncated[18..20].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(
            verifier
                .verify_mlp(&paths, [Ok(truncated)].into_iter())
                .unwrap_err()
                .contains("status -2")
        );
        let mut wrong = mlp(b"X");
        wrong[23] = 0xa0;
        assert!(
            verifier
                .verify_mlp(&paths, [Ok(wrong)].into_iter())
                .unwrap_err()
                .contains("status -3")
        );
    }
    #[test]
    fn reader_errors_and_malformed_sectors() {
        let source = Fixture::new(b"A");
        let paths = [source.0.clone()];
        let verifier = NativeDiscVerifier::load().unwrap();
        assert_eq!(
            verifier
                .verify_mlp(&paths, [Err("cancelled".into())].into_iter())
                .unwrap_err(),
            "cancelled"
        );
        assert_eq!(
            verifier
                .verify_mlp(&paths, [Ok(vec![0; 1])].into_iter())
                .unwrap_err(),
            "ISO verifier chunk is not sector aligned"
        );
        assert_eq!(
            verifier
                .verify_mlp(&paths, [Ok(vec![0; 2048])].into_iter())
                .unwrap_err(),
            "DVD audio payload verification failed: status -1, track 1, offset 0, sectors 0"
        );
        assert_eq!(
            verifier
                .verify_mlp(
                    &paths,
                    std::iter::once_with(|| -> Result<Vec<u8>, String> { panic!("reader") })
                )
                .unwrap_err(),
            "ISO reader callback panicked"
        );
    }
}

#[cfg(test)]
mod dll_parity {
    use super::*;
    use std::{
        ffi::{CString, OsStr, c_char, c_void},
        os::windows::ffi::OsStrExt,
        path::Path,
        ptr,
    };
    type ReadChunk = unsafe extern "C" fn(*mut c_void, *mut u8, u32) -> i32;
    type VerifyMlp = unsafe extern "C" fn(
        *const *const c_char,
        u32,
        ReadChunk,
        *mut c_void,
        *mut ResultInfo,
    ) -> i32;
    type VerifyLpcm = unsafe extern "C" fn(
        *const *const c_char,
        *const u8,
        u32,
        ReadChunk,
        *mut c_void,
        *mut ResultInfo,
    ) -> i32;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }
    struct Dll(*mut c_void);
    impl Drop for Dll {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
    unsafe extern "C" fn read(state: *mut c_void, out: *mut u8, capacity: u32) -> i32 {
        let state = unsafe { &mut *state.cast::<(&[u8], usize)>() };
        let count = state.0.len().saturating_sub(state.1).min(capacity as usize);
        unsafe {
            ptr::copy_nonoverlapping(state.0.as_ptr().add(state.1), out, count);
        }
        state.1 += count;
        count as i32
    }
    fn legacy(path: &Path, source: &Path, ends: Option<&[u8]>, sectors: &[u8]) -> ResultInfo {
        let wide: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
        let module = Dll(unsafe { LoadLibraryW(wide.as_ptr()) });
        assert!(
            !module.0.is_null(),
            "could not load legacy verifier: {}",
            path.display()
        );
        let name = if ends.is_some() {
            c"dvda_verify_lpcm_payload"
        } else {
            c"dvda_verify_mlp_payload"
        };
        let symbol = unsafe { GetProcAddress(module.0, name.as_ptr()) };
        assert!(!symbol.is_null());
        let source = CString::new(source.to_string_lossy().as_bytes()).unwrap();
        let sources = [source.as_ptr()];
        let mut state = (sectors, 0usize);
        let mut result = ResultInfo::default();
        unsafe {
            if let Some(ends) = ends {
                let verify: VerifyLpcm = std::mem::transmute(symbol);
                verify(
                    sources.as_ptr(),
                    ends.as_ptr(),
                    1,
                    read,
                    (&mut state as *mut (&[u8], usize)).cast(),
                    &mut result,
                );
            } else {
                let verify: VerifyMlp = std::mem::transmute(symbol);
                verify(
                    sources.as_ptr(),
                    1,
                    read,
                    (&mut state as *mut (&[u8], usize)).cast(),
                    &mut result,
                );
            }
        }
        result
    }
    fn find_dll() -> Option<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..\\..\\..");
        std::env::var_os("DVDA_DISC_VERIFY_LIBRARY")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                let path = root.join("build\\c-rust-migration-oracle\\dvda-disc-verify.dll");
                path.is_file().then_some(path)
            })
    }
    fn compare(dll: &Path, source: &super::tests::Fixture, ends: Option<&[u8]>, sectors: Vec<u8>) {
        let result = legacy(dll, &source.0, ends, &sectors);
        let verifier = NativeDiscVerifier::load().unwrap();
        let actual = match ends {
            Some(ends) => verifier.verify_lpcm(
                std::slice::from_ref(&source.0),
                ends,
                [Ok(sectors)].into_iter(),
            ),
            None => verifier.verify_mlp(std::slice::from_ref(&source.0), [Ok(sectors)].into_iter()),
        };
        match actual {
            Ok(actual) => {
                assert_eq!(result.code, 0);
                assert_eq!(
                    (actual.track, actual.offset, actual.sectors, actual.bytes),
                    (result.track, result.offset, result.sectors, result.bytes)
                );
            }
            Err(message) => assert!(
                message.contains(&format!(
                    "status {}, track {}, offset {}, sectors {}",
                    result.code,
                    result.track + 1,
                    result.offset,
                    result.sectors
                )),
                "{message}"
            ),
        }
    }
    #[test]
    fn differential_against_optional_original_dll() {
        let Some(dll) = find_dll() else {
            eprintln!("No legacy verifier DLL found; skipping differential fixtures");
            return;
        };
        let source = super::tests::Fixture::new(b"ABC");
        let valid = super::tests::mlp(b"ABC");
        compare(&dll, &source, None, valid.clone());
        compare(&dll, &source, None, super::tests::mlp(b"ABD"));
        compare(&dll, &source, None, super::tests::mlp(b"AB"));
        compare(&dll, &source, None, super::tests::mlp(b"ABCD"));
        for (position, value) in [
            (0, 1),
            (4, 0),
            (14, 1),
            (17, 0),
            (19, 0),
            (20, 0),
            (23, 0xa0),
            (26, 0),
            (33, 0),
        ] {
            let mut changed = valid.clone();
            changed[position] = value;
            compare(&dll, &source, None, changed);
        }
        let wav = super::tests::Fixture::new(&super::tests::wave(&[1, 2, 3, 4]));
        let valid_pcm = super::tests::lpcm(&[2, 1, 4, 3, 0, 0, 0, 0]);
        compare(&dll, &wav, Some(&[1]), valid_pcm.clone());
        compare(&dll, &wav, Some(&[0]), valid_pcm.clone());
        for (position, value) in [
            (4, 0),
            (19, 0),
            (23, 0xa1),
            (26, 0),
            (30, 0),
            (31, 0),
            (33, 0),
            (35, 0),
        ] {
            let mut changed = valid_pcm.clone();
            changed[position] = value;
            compare(&dll, &wav, Some(&[1]), changed);
        }
        for seed in 0..256usize {
            let mut changed = if seed & 1 == 0 {
                valid.clone()
            } else {
                valid_pcm.clone()
            };
            let position = (seed.wrapping_mul(7919) + seed / 3) % changed.len();
            changed[position] ^= (seed as u8).wrapping_mul(41).wrapping_add(1);
            if seed & 1 == 0 {
                compare(&dll, &source, None, changed);
            } else {
                compare(&dll, &wav, Some(&[1]), changed);
            }
        }
    }
}
