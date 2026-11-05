//! Colour remapping corresponding to dvdvob.c's remapcolor/remapbyte/procremap.
use super::dvdata::DvdData;

pub(crate) struct ColorRemap {
    colors: [u32; 16],
    palette: [u32; 16],
    state: u8,
    offset: usize,
    length: usize,
    next: usize,
    skip: usize,
    line: u32,
}

impl ColorRemap {
    pub(crate) fn new(data: &DvdData, palette: [u32; 16]) -> Self {
        let mut colors = std::array::from_fn(|i| i as u32);
        for (index, color) in data.palette.iter().enumerate().take(16) {
            colors[index] = 0x0100_0000
                | (u32::from(color.y) << 16)
                | (u32::from(color.cr) << 8)
                | u32::from(color.cb);
        }
        Self {
            colors,
            palette,
            state: 0,
            offset: 0,
            length: 0,
            next: 0,
            skip: 0,
            line: 0,
        }
    }

    fn remapcolor(&mut self, index: usize) -> Result<u8, String> {
        if self.colors[index] < 16 {
            return Ok(self.colors[index] as u8);
        }
        let color = self.colors[index] & 0x00ff_ffff;
        let index_out = self
            .palette
            .iter()
            .position(|&v| v == color)
            .or_else(|| self.palette.iter().position(|&v| v == 0x0100_0000))
            .ok_or("color map full, unable to allocate new colors")?;
        self.palette[index_out] = color;
        self.colors[index] = index_out as u32;
        Ok(index_out as u8)
    }

    fn remapbyte(&mut self, byte: &mut u8) -> Result<(), String> {
        let low = self.remapcolor(usize::from(*byte & 15))?;
        *byte = low | (self.remapcolor(usize::from(*byte >> 4))? << 4);
        Ok(())
    }

    pub(crate) fn groups(&mut self, data: &mut DvdData) -> Result<(), String> {
        for group in &mut data.groups {
            for index in [0, 2] {
                let mut bytes = group.entries[index].to_be_bytes();
                for byte in &mut bytes {
                    self.remapbyte(byte)?;
                }
                group.entries[index] = u16::from_be_bytes(bytes);
            }
        }
        Ok(())
    }

    pub(crate) fn procremap(&mut self, bytes: &mut [u8], timespan: &mut u64) -> Result<(), String> {
        for byte in bytes {
            if self.state == 4 {
                if self.offset == self.length {
                    self.state = 0;
                } else if self.offset == self.next {
                    self.state = 5;
                }
            }
            if self.state == 9 && self.skip == 0 {
                self.state = 10;
            }
            match self.state {
                0 => {
                    self.offset = 0;
                    self.skip = 0;
                    self.length = usize::from(*byte) * 256;
                    self.state = 1;
                }
                1 => {
                    self.length += usize::from(*byte);
                    self.state = 2;
                }
                2 => {
                    self.next = usize::from(*byte) * 256;
                    self.state = 3;
                }
                3 => {
                    self.next += usize::from(*byte);
                    self.state = 4;
                }
                4 => {}
                5 => {
                    *timespan += 1024 * u64::from(*byte) * 256;
                    self.state = 6;
                }
                6 => {
                    *timespan += 1024 * u64::from(*byte);
                    self.state = 7;
                }
                7 => {
                    self.next = usize::from(*byte) * 256;
                    self.state = 8;
                }
                8 => {
                    self.next += usize::from(*byte);
                    self.state = 10;
                }
                9 => {
                    self.skip -= 1;
                }
                10 => match *byte {
                    0..=2 => {}
                    3 => self.state = 11,
                    4..=6 => {
                        self.skip = match *byte {
                            4 => 2,
                            5 => 6,
                            _ => 4,
                        };
                        self.state = 9;
                    }
                    7 => {
                        self.skip = 2;
                        self.state = 13;
                    }
                    255 => self.state = 4,
                    command => return Err(format!("unknown subtitle command: {command}")),
                },
                11 => {
                    self.remapbyte(byte)?;
                    self.state = 12;
                }
                12 => {
                    self.remapbyte(byte)?;
                    self.state = 10;
                }
                13 => {
                    self.skip -= 1;
                    if self.skip == 0 {
                        self.state = 14;
                    }
                }
                14 => {
                    self.line = u32::from(*byte) << 24;
                    self.state = 15;
                }
                15 => {
                    self.line |= u32::from(*byte) << 16;
                    self.state = 16;
                }
                16 => {
                    self.line |= u32::from(*byte) << 8;
                    self.state = 17;
                }
                17 => {
                    self.line |= u32::from(*byte);
                    if self.line == 0x0fff_ffff {
                        self.state = 10;
                    } else {
                        self.line = (self.line >> 12) & 15;
                        self.skip = 2;
                        self.state = 18;
                    }
                }
                18 => {
                    self.skip -= 1;
                    if self.skip == 0 {
                        self.skip = 2;
                        self.state = 19;
                    }
                }
                19 => {
                    self.remapbyte(byte)?;
                    self.skip -= 1;
                    if self.skip == 0 {
                        self.skip = 2;
                        self.state = 20;
                    }
                }
                20 => {
                    self.skip -= 1;
                    if self.skip == 0 {
                        self.line = self
                            .line
                            .checked_sub(1)
                            .ok_or("empty subtitle color-control line")?;
                        if self.line == 0 {
                            self.state = 14;
                        } else {
                            self.skip = 2;
                            self.state = 18;
                        }
                    }
                }
                _ => unreachable!(),
            }
            self.offset += 1;
        }
        Ok(())
    }
}

pub(crate) fn remap(
    output: &mut [u8],
    data: &mut DvdData,
    palette: [u32; 16],
) -> Result<(), String> {
    let mut remap = ColorRemap::new(data, palette);
    remap.groups(data)?;
    let mut timespan = 0;
    for sector in output.as_chunks_mut::<2048>().0 {
        let mut at = 14 + usize::from(sector[13] & 7);
        while at + 6 <= sector.len() && sector[at..at + 3] == [0, 0, 1] {
            let end = at + 6 + usize::from(u16::from_be_bytes([sector[at + 4], sector[at + 5]]));
            if end > sector.len() {
                return Err("PES packet exceeds sector".into());
            }
            if sector[at + 3] == 0xbd && at + 9 < end {
                let payload = at + 9 + usize::from(sector[at + 8]);
                if payload < end && sector[payload] == 0x20 {
                    remap.procremap(&mut sector[payload + 1..end], &mut timespan)?;
                }
            }
            at = end;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::dvdata::{Colour, Group};
    use super::*;

    #[test]
    fn remaps_highlights_and_split_spu_commands_without_changing_alpha() {
        let mut data = DvdData {
            palette: vec![
                Colour {
                    y: 16,
                    cr: 128,
                    cb: 128,
                },
                Colour {
                    y: 82,
                    cr: 240,
                    cb: 90,
                },
                Colour {
                    y: 235,
                    cr: 128,
                    cb: 128,
                },
            ],
            groups: vec![Group {
                entries: [0x2100, 0xfff0, 0x0100, 0x0ff0],
            }],
            ..DvdData::default()
        };
        let mut palette = [0x108080; 16];
        palette[0] = 0xeb8080;
        palette[2] = 0x52f05a;
        let mut remap = ColorRemap::new(&data, palette);
        remap.groups(&mut data).unwrap();
        assert_eq!(data.groups[0].entries, [0x0211, 0xfff0, 0x1211, 0x0ff0]);
        let mut spu = [0, 12, 0, 4, 0, 1, 0, 4, 3, 0x21, 0x00, 255];
        let mut time = 0;
        for chunk in spu.chunks_mut(1) {
            remap.procremap(chunk, &mut time).unwrap();
        }
        assert_eq!(spu, [0, 12, 0, 4, 0, 1, 0, 4, 3, 0x02, 0x11, 255]);
        assert_eq!(time, 1024);
        let mut changed = [
            0, 26, 0, 4, 0, 0, 0, 4, 7, 0, 16, 0, 0, 0x10, 0, 0, 7, 0x21, 0x00, 0xff, 0xf0, 0x0f,
            0xff, 0xff, 0xff, 255,
        ];
        for chunk in changed.chunks_mut(3) {
            remap.procremap(chunk, &mut time).unwrap();
        }
        assert_eq!(&changed[15..21], &[0, 7, 0x02, 0x11, 0xff, 0xf0]);
        let mut next = [0, 12, 0, 4, 0, 0, 0, 4, 3, 0x21, 0x00, 255];
        remap.procremap(&mut next, &mut time).unwrap();
        assert_eq!(&next[9..11], &[0x02, 0x11]);
    }
}
