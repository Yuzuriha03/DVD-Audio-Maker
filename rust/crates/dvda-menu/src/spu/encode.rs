//! DVD subpicture run-length encoder (port of the spumux DVD encoder).
//!
//! The bit writer, RLE codes and control-block layout are reproduced exactly so
//! the resulting subpicture unit matches the historical C encoder byte for byte.

use super::image::SubPicture;

pub(crate) const SUB_BUFFER_MAX: usize = 53220;
const HEADROOM: usize = 1024;

const SPU_FSTA_DSP: u32 = 0;
const SPU_STA_DSP: u32 = 1;
const SPU_STP_DSP: u32 = 2;
const SPU_SET_COLOR: u32 = 3;
const SPU_SET_CONTR: u32 = 4;
const SPU_SET_DAREA: u32 = 5;
const SPU_SET_DSPXA: u32 = 6;
const SPU_CMD_END: u32 = 255;

struct BitWriter {
    data: Vec<u8>,
    position: usize,
    remain: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self { data: vec![0; SUB_BUFFER_MAX + HEADROOM], position: 0, remain: 8 }
    }

    fn bits(&mut self, value: u32, mut bits: u32) {
        const MASK: [u32; 9] = [0, 1, 3, 7, 15, 31, 63, 127, 255];
        while bits != 0 {
            if bits > self.remain {
                self.data[self.position] |= ((value >> (bits - self.remain)) & MASK[self.remain as usize]) as u8;
                self.position += 1;
                bits -= self.remain;
                self.remain = 8;
            } else if bits == self.remain {
                self.data[self.position] |= (value & MASK[self.remain as usize]) as u8;
                self.position += 1;
                self.remain = 8;
                return;
            } else {
                self.data[self.position] |= ((value & MASK[bits as usize]) << (self.remain - bits)) as u8;
                self.remain -= bits;
                return;
            }
        }
    }

    fn nibble(&mut self, value: u32) {
        self.bits(value, 4);
    }

    fn byte(&mut self, value: u32) {
        self.bits(value, 8);
    }

    fn word(&mut self, value: u32) {
        self.bits(value, 16);
    }

    fn trinibble(&mut self, value: u32) {
        self.nibble((value >> 8) & 15);
        self.nibble((value >> 4) & 15);
        self.nibble(value & 15);
    }

    fn align(&mut self) {
        if self.remain != 8 {
            let remain = self.remain;
            self.bits(0, remain);
        }
    }

    fn set_word(&mut self, at: usize, value: u32) {
        self.data[at] = (value >> 8) as u8;
        self.data[at + 1] = value as u8;
    }
}

fn rle(writer: &mut BitWriter, count: i32, color: u8) {
    debug_assert!((1..=255).contains(&count));
    let value = ((count as u32) << 2) | u32::from(color);
    if count >= 64 {
        writer.nibble(0);
        writer.nibble((value & 0xF00) >> 8);
        writer.nibble((value & 0xF0) >> 4);
        writer.nibble(value & 0xF);
    } else if count >= 16 {
        writer.nibble(0);
        writer.nibble((value & 0xF0) >> 4);
        writer.nibble(value & 0xF);
    } else if count >= 4 {
        writer.nibble((value & 0xF0) >> 4);
        writer.nibble(value & 0xF);
    } else {
        writer.nibble(value & 0xF);
    }
}

fn encode_row(writer: &mut BitWriter, picture: &SubPicture, y: usize) -> Result<(), String> {
    let xd = picture.xd as usize;
    let row = &picture.fimg[y * xd..y * xd + xd];
    let started = writer.position;
    let mut next_run = 0usize;
    for x in 0..xd.saturating_sub(1) {
        if row[x + 1] != row[x] {
            let mut count = (x + 1 - next_run) as i32;
            while count > 255 {
                rle(writer, 255, row[x]);
                count -= 255;
            }
            if count != 0 {
                rle(writer, count, row[x]);
            }
            next_run = x + 1;
        }
    }
    if xd != next_run {
        let count = (xd - next_run) as i32;
        if count < 64 {
            rle(writer, count, row[next_run]);
        } else {
            writer.nibble(0);
            writer.nibble(0);
            writer.nibble(0);
            writer.nibble(u32::from(row[next_run]));
        }
    }
    writer.align();
    if writer.position - started >= 1440 / 8 {
        return Err("encoded row takes more than 1440 bits".into());
    }
    Ok(())
}

pub(crate) fn dvd_encode(picture: &mut SubPicture) -> Result<Vec<u8>, String> {
    let xstart = picture.x0;
    let xsize = picture.xd;
    let ystart = picture.y0;
    let ysize = picture.yd;

    let mut writer = BitWriter::new();
    writer.position = 4;

    let offset0 = writer.position as u32;
    let mut y = 0usize;
    while y < ysize as usize {
        encode_row(&mut writer, picture, y)?;
        y += 2;
    }
    let offset1 = writer.position as u32;
    y = 1;
    while y < ysize as usize {
        encode_row(&mut writer, picture, y)?;
        y += 2;
    }

    let control = writer.position as u32;
    writer.set_word(2, control);
    writer.word(0);
    let mut next_command = writer.position;
    writer.position += 2;

    writer.byte(if picture.forced { SPU_FSTA_DSP } else { SPU_STA_DSP });

    writer.byte(SPU_SET_COLOR);
    let p3 = picture.find_master_pal(picture.pal[3]);
    let p2 = picture.find_master_pal(picture.pal[2]);
    let p1 = picture.find_master_pal(picture.pal[1]);
    let p0 = picture.find_master_pal(picture.pal[0]);
    writer.nibble(p3 as u32);
    writer.nibble(p2 as u32);
    writer.nibble(p1 as u32);
    writer.nibble(p0 as u32);

    writer.byte(SPU_SET_CONTR);
    writer.nibble(u32::from(picture.pal[3].a >> 4));
    writer.nibble(u32::from(picture.pal[2].a >> 4));
    writer.nibble(u32::from(picture.pal[1].a >> 4));
    writer.nibble(u32::from(picture.pal[0].a >> 4));

    writer.byte(SPU_SET_DAREA);
    writer.trinibble(xstart);
    writer.trinibble(xstart + xsize - 1);
    writer.trinibble(ystart);
    writer.trinibble(ystart + ysize - 1);

    writer.byte(SPU_SET_DSPXA);
    writer.word(offset0);
    writer.word(offset1);

    writer.byte(SPU_CMD_END);

    if picture.sd >= 0 {
        let mut duration = (picture.sd + 512) / 1024;
        writer.set_word(next_command, writer.position as u32);
        next_command = writer.position;
        while duration >= 65536 {
            writer.word(65535);
            writer.word((next_command + 5) as u32);
            writer.byte(SPU_CMD_END);
            next_command = writer.position;
            duration -= 65535;
        }
        writer.word(duration as u32);
        writer.word(next_command as u32);
        writer.byte(SPU_STP_DSP);
        writer.byte(SPU_CMD_END);
    } else {
        writer.set_word(next_command, (next_command as u32).wrapping_sub(2));
    }

    if writer.position & 1 != 0 {
        writer.byte(SPU_CMD_END);
    }

    let size = writer.position;
    writer.set_word(0, size as u32);
    if size >= SUB_BUFFER_MAX {
        return Err("subpicture too large".into());
    }
    writer.data.truncate(size);
    Ok(writer.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spu::color::Color;

    #[test]
    fn bit_writer_packs_msb_first() {
        let mut writer = BitWriter::new();
        writer.bits(0b101, 3);
        writer.bits(0b11, 2);
        writer.bits(0b1111, 4);
        writer.align();
        assert_eq!(&writer.data[..2], &[0b1011_1111, 0b1000_0000]);
    }

    #[test]
    fn rle_lengths_match_layout() {
        let mut writer = BitWriter::new();
        rle(&mut writer, 1, 2);
        rle(&mut writer, 5, 1);
        rle(&mut writer, 20, 3);
        rle(&mut writer, 70, 0);
        writer.align();
        assert_eq!(&writer.data[..5], &[0x61, 0x50, 0x53, 0x01, 0x18]);
        let _ = Color::TRANSPARENT;
    }
}
