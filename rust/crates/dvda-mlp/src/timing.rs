use crate::Error;
pub(crate) const LOOKAHEAD: usize = 1200;
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pair {
    minimum: i32,
    total: i32,
}
fn combine(a: Pair, b: Pair) -> Pair {
    Pair {
        minimum: a.minimum.min(a.total.wrapping_add(b.minimum)),
        total: a.total.wrapping_add(b.total),
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Descriptor {
    pub(crate) rate: u32,
    pub(crate) samples: u32,
    pub(crate) decode: u32,
    pub(crate) words: u32,
    pub(crate) groups: [u32; 2],
    pub(crate) arrival: u32,
    pub(crate) header: u32,
}
fn duration(d: Descriptor) -> u32 {
    (d.words * 256).div_ceil(d.rate & 0x7fff).max(d.samples / 4)
}
#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Timing {
    tree: [Pair; LOOKAHEAD],
    aggregate: Pair,
    cursor: i32,
    previous: i32,
    write: u32,
    read: u32,
    times: [u32; 100],
    sizes: [[u32; 3]; 100],
    totals: [u32; 3],
}
impl Timing {
    pub(crate) fn new() -> Self {
        Self {
            tree: [Pair {
                minimum: i32::MAX,
                total: 0,
            }; LOOKAHEAD],
            aggregate: Pair {
                minimum: i32::MAX,
                total: 0,
            },
            cursor: LOOKAHEAD as i32 - 1,
            previous: i32::MIN,
            write: 0,
            read: 0,
            times: [0; 100],
            sizes: [[0; 3]; 100],
            totals: [0; 3],
        }
    }
    fn buffer(&mut self, d: Descriptor, mut delay: i32) -> i32 {
        let mut write = self.write;
        let mut read = self.read;
        self.times[write as usize] = d.decode.wrapping_add(d.samples).wrapping_add(1);
        self.sizes[write as usize] = [d.words, d.groups[0], d.groups[1]];
        for i in 0..3 {
            self.totals[i] = self.totals[i].wrapping_add(self.sizes[write as usize][i]);
        }
        write = (write + 1) % 100;
        if write == read {
            self.write = 0;
            self.read = 0;
            self.totals = [0; 3];
        }
        loop {
            let full = self.totals[0] >= 45000
                || (d.groups[1] != 0 && (self.totals[1] >= 15000 || self.totals[2] >= 30000));
            let expired = write != read
                && (d
                    .decode
                    .wrapping_sub(self.times[read as usize])
                    .wrapping_add(delay as u32)
                    & 65535)
                    <= 32767;
            if !expired {
                if !full {
                    break;
                }
                delay = (self.times[read as usize].wrapping_sub(d.decode) & 65535) as i32 - 65536;
            }
            for i in 0..3 {
                self.totals[i] = self.totals[i].wrapping_sub(self.sizes[read as usize][i]);
            }
            read = (read + 1) % 100;
        }
        self.write = write;
        self.read = read;
        delay
    }
    pub(crate) fn step(&mut self, next: Descriptor, d: &mut Descriptor) -> Result<(), Error> {
        let total = if next.words != 0 {
            next.samples as i32 - duration(next) as i32
        } else {
            80
        };
        let pair = Pair {
            total,
            minimum: total.min(0),
        };
        let range = if self.cursor < 0 {
            self.aggregate = pair;
            for i in 1..LOOKAHEAD {
                self.tree[i] = combine(self.tree[i], self.tree[i - 1]);
            }
            self.cursor = LOOKAHEAD as i32 - 1;
            combine(self.tree[self.cursor as usize], self.aggregate)
        } else {
            self.tree[self.cursor as usize] = pair;
            self.cursor -= 1;
            self.aggregate = combine(self.aggregate, pair);
            if self.cursor < 0 {
                self.aggregate
            } else {
                combine(self.tree[self.cursor as usize], self.aggregate)
            }
        };
        if d.words == 0 {
            self.previous = i32::MIN;
            return Ok(());
        }
        let upper = range.minimum.wrapping_sub(d.samples as i32);
        let mut lower = range
            .total
            .wrapping_sub(d.samples as i32 * 90)
            .max(-(d.samples as i32) * 90);
        let previous = self.buffer(*d, self.previous);
        let failed = upper < previous;
        if failed {
            lower = upper;
        } else {
            lower = lower.max(previous).min(upper);
        }
        self.previous = lower
            .wrapping_add(duration(*d) as i32 - d.samples as i32)
            .max(-(d.samples as i32) * 90);
        let mut arrival = d.decode.wrapping_add(lower as u32) & 65535;
        if d.rate & 0x8000 == 0 {
            arrival = arrival.wrapping_sub(15 / d.rate).wrapping_sub(1) & 65535;
        }
        let mut delta = d.arrival ^ arrival;
        delta ^= ((delta as i32) >> 8) as u32;
        d.header ^= (((delta & 15) << 4) ^ (delta & 240)) << 8;
        d.arrival = arrival;
        if failed {
            return Err(Error::FifoOverflow);
        }
        Ok(())
    }
}
/// Holds at most the timing lookahead plus the AU currently being delivered.
#[cfg(not(target_arch = "x86_64"))]
pub(crate) struct Queue {
    timing: Timing,
    pending: std::collections::VecDeque<(Descriptor, Vec<u8>)>,
    rate: u32,
    decode: u32,
    samples: u32,
    primed: usize,
}
#[cfg(not(target_arch = "x86_64"))]
impl Queue {
    pub(crate) fn new(samples: u32) -> Self {
        Self {
            timing: Timing::new(),
            pending: std::collections::VecDeque::new(),
            rate: 0,
            decode: 0,
            samples,
            primed: 0,
        }
    }
    fn advance(&mut self, next: Descriptor) -> Result<Option<Vec<u8>>, Error> {
        if self.primed < LOOKAHEAD {
            self.timing.step(next, &mut Descriptor::default())?;
            self.primed += 1;
            return Ok(None);
        }
        let Some((mut descriptor, mut bytes)) = self.pending.pop_front() else {
            return Ok(None);
        };
        self.timing.step(next, &mut descriptor)?;
        bytes[..2].copy_from_slice(&(descriptor.header as u16).to_be_bytes());
        bytes[2..4].copy_from_slice(&(descriptor.arrival as u16).to_be_bytes());
        Ok(Some(bytes))
    }
    pub(crate) fn push(&mut self, bytes: Vec<u8>) -> Result<Option<Vec<u8>>, Error> {
        let word = |i: usize| u32::from(u16::from_be_bytes([bytes[i * 2], bytes[i * 2 + 1]]));
        let major = word(2) == 0xf872 && word(3) == 0x6fbb;
        let directory = if major {
            self.rate = word(9);
            16
        } else {
            2
        };
        let descriptor = Descriptor {
            rate: self.rate,
            samples: self.samples,
            decode: self.decode,
            words: word(0) & 4095,
            groups: [word(directory) & 4095, 0],
            arrival: word(1),
            header: word(0),
        };
        self.decode = (self.decode + self.samples) & 65535;
        let ready = self.advance(descriptor)?;
        self.pending.push_back((descriptor, bytes));
        Ok(ready)
    }
    pub(crate) fn finish(&mut self) -> Result<Option<Vec<u8>>, Error> {
        while self.primed < LOOKAHEAD {
            self.advance(Descriptor::default())?;
        }
        self.advance(Descriptor::default())
    }
}
/// Port of the native 1200-AU lookahead and decoder FIFO admission state machine.
pub fn rewrite(bytes: &mut [u8], samples: u32) -> Result<(), Error> {
    let mut descriptors = Vec::new();
    let mut offsets = Vec::new();
    let mut at = 0;
    let mut rate = 0;
    let mut decode = 0;
    while at < bytes.len() {
        let word = |i: usize| {
            u32::from(u16::from_be_bytes([
                bytes[at + i * 2],
                bytes[at + i * 2 + 1],
            ]))
        };
        let header = word(0);
        let words = header & 4095;
        let major = word(2) == 0xf872 && word(3) == 0x6fbb;
        let directory = if major {
            rate = word(9);
            16
        } else {
            2
        };
        let first = word(directory) & 4095;
        descriptors.push(Descriptor {
            rate,
            samples,
            decode,
            words,
            groups: [first, 0],
            arrival: word(1),
            header,
        });
        offsets.push(at);
        at += words as usize * 2;
        decode = (decode + samples) & 65535;
    }
    let mut timing = Timing::new();
    let empty = Descriptor::default();
    for i in 0..LOOKAHEAD {
        let mut dummy = empty;
        timing.step(descriptors.get(i).copied().unwrap_or(empty), &mut dummy)?;
    }
    for i in 0..descriptors.len() {
        let next = descriptors.get(i + LOOKAHEAD).copied().unwrap_or(empty);
        timing.step(next, &mut descriptors[i])?;
        let at = offsets[i];
        bytes[at..at + 2].copy_from_slice(&(descriptors[i].header as u16).to_be_bytes());
        bytes[at + 2..at + 4].copy_from_slice(&(descriptors[i].arrival as u16).to_be_bytes());
    }
    Ok(())
}
