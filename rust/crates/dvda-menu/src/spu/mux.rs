//! MPEG program-stream copy and subpicture insertion (port of `menu_spu_core`).
//!
//! Reproduces the historical multiplexer byte for byte: PACK/PES headers, the
//! private dvdauthor-data sector and the subpicture packets are written in the
//! same order with the same timing state machine.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::decimal_bitwise_operands,
    clippy::too_many_lines,
    reason = "MPEG PACK/PES fields retain their protocol widths and C-compatible scheduling arithmetic"
)]
use super::encode::dvd_encode;
use super::format::VideoFormat;
use super::image::{RgbaReader, SubPicture};
use super::xml::Document;
use std::io::{Read, Write};
use std::path::Path;

const DVD_RATE: u64 = 43886;
const SECTOR_SIZE: usize = 2048;
const SUB_STREAM: u8 = 0x20;
const MPID_PACK: u32 = 0xba;
const MPID_SYSTEM: u32 = 0xbb;
const MPID_PROGRAM_END: u32 = 0xb9;
const MPID_PRIVATE1: u32 = 0xbd;
const MPID_PAD: u32 = 0xbe;
const MPID_VIDEO_FIRST: u32 = 0xe0;
const MPID_VIDEO_LAST: u32 = 0xef;
const TBS: i32 = 90;

fn low_byte(value: u64) -> u8 {
    value.to_le_bytes()[0]
}

fn pack_header(header: &mut [u8; 32], time: u64, muxrate: u32, stuffing: u8) {
    let th = time / 300;
    let tl = time % 300;
    header[0] = 0x44 | low_byte((th >> 27) & 0x38) | low_byte((th >> 28) & 3);
    header[1] = low_byte(th >> 20);
    header[2] = 4 | low_byte((th >> 12) & 0xf8) | low_byte((th >> 13) & 3);
    header[3] = low_byte(th >> 5);
    header[4] = 4 | low_byte((th & 31) << 3) | low_byte((tl >> 7) & 3);
    header[5] = 1 | low_byte(tl << 1);
    header[6] = low_byte(u64::from(muxrate >> 14));
    header[7] = low_byte(u64::from(muxrate >> 6));
    header[8] = 3 | low_byte(u64::from(muxrate << 2));
    header[9] = 0xF8 | stuffing;
}

fn pes_header0(header: &mut [u8; 32], pts: u32) {
    let pts = u64::from(pts);
    header[0] = 0x81;
    header[1] = 0x80;
    header[2] = 5;
    header[3] = 0x21 | low_byte((pts >> 29) & 6);
    header[4] = low_byte(pts >> 22);
    header[5] = 1 | low_byte(pts >> 14);
    header[6] = low_byte(pts >> 7);
    header[7] = 1 | low_byte(pts << 1);
}

fn pes_header1(header: &mut [u8; 32], pts: u32) {
    pes_header0(header, pts);
    header[1] = 0x81;
    header[2] = 8;
    header[8] = 0x1e;
    header[9] = 0x60;
    header[10] = 0x3a;
}

fn pes_header2(header: &mut [u8; 32]) {
    header[0] = 0x81;
    header[1] = 0;
    header[2] = 0;
}

fn get_muxrate(buffer: &[u8]) -> u32 {
    u32::from(buffer[8] >> 2) | (u32::from(buffer[7]) * 64) | (u32::from(buffer[6]) * 16384)
}

fn get_gts(buffer: &[u8]) -> Option<u64> {
    if (buffer[8] & 3) != 3
        || (buffer[5] & 1) != 1
        || (buffer[4] & 4) != 4
        || (buffer[2] & 4) != 4
        || (buffer[0] & 0xc4) != 0x44
    {
        return None;
    }
    let th = u64::from(buffer[4] >> 3)
        + u64::from(buffer[3]) * 32
        + u64::from(buffer[2] & 3) * 32 * 256
        + u64::from(buffer[2] & 0xf8) * 32 * 128
        + u64::from(buffer[1]) * 1024 * 1024
        + u64::from(buffer[0] & 3) * 1024 * 1024 * 256
        + u64::from(buffer[0] & 0x38) * 1024 * 1024 * 128;
    let tl = u64::from((buffer[4] & 3) << 7) | u64::from(buffer[5] >> 1);
    Some(th * 300 + tl)
}

fn fix_gts(gts: &mut u64, nextgts: &mut u64) {
    if *gts < *nextgts {
        *gts = *nextgts;
    }
    *nextgts = *gts + DVD_RATE;
}

fn get_pts(buffer: &[u8]) -> Option<u32> {
    let buffer = buffer.get(..8)?;
    if (buffer[1] & 0xc0) == 0
        || buffer[2] < 4
        || (buffer[3] & 0xe1) != 0x21
        || (buffer[5] & 1) != 1
        || (buffer[7] & 1) != 1
    {
        return None;
    }
    Some(
        u32::from(buffer[7] >> 1)
            + u32::from(buffer[6]) * 128
            + u32::from(buffer[5] & 254) * 16384
            + u32::from(buffer[4]) * 16384 * 256
            + u32::from(buffer[3] & 14) * 16384 * 256 * 128,
    )
}

fn private_substream(payload: &[u8]) -> Result<Option<u8>, String> {
    let offset = usize::from(*payload.get(2).ok_or("truncated private PES header")?) + 3;
    let substream = *payload.get(offset).ok_or("truncated private PES payload")?;
    Ok(((substream & 0xE0) != 0x80).then_some(substream))
}

fn be32(value: u32) -> [u8; 4] {
    value.to_be_bytes()
}

fn be16(value: u16) -> [u8; 2] {
    value.to_be_bytes()
}

fn write_string(data: &mut [u8], at: &mut usize, text: &str) {
    for byte in text.bytes() {
        data[*at] = byte;
        *at += 1;
    }
    data[*at] = 0;
    *at += 1;
}

fn write_short(data: &mut [u8], at: &mut usize, value: u16) {
    data[*at..*at + 2].copy_from_slice(&value.to_be_bytes());
    *at += 2;
}

fn write_long(data: &mut [u8], at: &mut usize, value: u32) {
    data[*at..*at + 4].copy_from_slice(&value.to_be_bytes());
    *at += 4;
}

struct Mux {
    spus: Vec<SubPicture>,
    index: usize,
    next: Option<SubPicture>,
    tofs: i32,
    lastgts: u64,
    nextgts: u64,
    muxrate: u32,
    header_size: usize,
    subno: i32,
    skipped: i32,
    lps: u64,
    sector: [u8; SECTOR_SIZE],
    output: Vec<u8>,
    input: std::fs::File,
}

impl Mux {
    fn new(input: std::fs::File, document: Document) -> Self {
        Self {
            spus: document.spus,
            index: 0,
            next: None,
            tofs: -1,
            lastgts: 0,
            nextgts: 0,
            muxrate: 10080 * 10 / 4,
            header_size: 12,
            subno: -1,
            skipped: 0,
            lps: 0,
            sector: [0; SECTOR_SIZE],
            output: Vec::new(),
            input,
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.lps += bytes.len() as u64;
        self.output.extend_from_slice(bytes);
    }

    fn write_start(&mut self, code: u32) {
        self.write(&be32(0x100 + code));
    }

    fn get_next_sub(&mut self, rgba: &mut RgbaReader<'_>) -> Option<SubPicture> {
        while self.index < self.spus.len() {
            let mut spu = std::mem::take(&mut self.spus[self.index]);
            self.index += 1;
            if self.tofs > 0 {
                spu.spts += self.tofs;
            }
            match spu.process(rgba) {
                Ok(()) => return Some(spu),
                Err(_error) => self.skipped += 1,
            }
        }
        None
    }

    fn due_gts(spu: &SubPicture) -> i64 {
        let due = (f64::from(spu.spts) - 0.15 * 90000.0) * 300.0;
        if due < 0.0 { 0 } else { due as i64 }
    }

    fn mux_next(&mut self, eoinput: bool, rgba: &mut RgbaReader<'_>) -> Result<(), String> {
        if self.lastgts == 0
            || self.tofs == -1
            || (!self.lps.is_multiple_of(SECTOR_SIZE as u64) && !eoinput)
        {
            return Ok(());
        }
        while let Some(mut current) = self.next.take() {
            let duegt = Self::due_gts(&current);
            if duegt > self.lastgts as i64 && !eoinput {
                self.next = Some(current);
                break;
            }
            self.next = self.get_next_sub(rgba);
            if let Some(next) = &self.next
                && i64::from(current.spts) + i64::from(current.sd) + i64::from(TBS)
                    > i64::from(next.spts)
            {
                current.sd = -1;
            }
            if current.sd == -1
                && let Some(next) = &self.next
            {
                if next.spts > current.spts + TBS {
                    current.sd = next.spts - current.spts - TBS;
                } else {
                    self.skipped += 1;
                    continue;
                }
            }
            let sub_size = match dvd_encode(&mut current) {
                Ok(size) => size,
                Err(error) if error.contains("too large") => {
                    self.skipped += 1;
                    continue;
                }
                Err(error) => return Err(error),
            };
            self.subno += 1;
            self.lastgts = duegt as u64;
            self.write_dvdauthor_data(&mut current);
            self.write_subpicture(&current, &sub_size);
        }
        Ok(())
    }

    fn write_dvdauthor_data(&mut self, spu: &mut SubPicture) {
        let pdl = SECTOR_SIZE - 6 - 10 - 4;
        self.write_start(MPID_PACK);
        let mut header = [0u8; 32];
        pack_header(&mut header, self.lastgts, self.muxrate, 0);
        fix_gts(&mut self.lastgts, &mut self.nextgts);
        self.write(&header[..10]);
        self.write(&[0, 0, 1, MPID_PAD as u8]);
        self.write(&[(pdl >> 8) as u8, pdl as u8]);
        let sector = &mut self.sector;
        sector[..pdl].fill(0xff);
        let mut at = 0usize;
        write_string(sector, &mut at, "dvdauthor-data");
        sector[at] = 2;
        at += 1;
        sector[at] = 1;
        at += 1;
        sector[at] = SUB_STREAM;
        at += 1;
        write_long(sector, &mut at, spu.spts as u32);
        write_long(
            sector,
            &mut at,
            if spu.sd == -1 {
                u32::MAX
            } else {
                (spu.sd + spu.spts) as u32
            },
        );
        sector[at] = 1;
        at += 1;
        sector[at] = u8::try_from(spu.numpal).expect("master palette is limited to 16 entries");
        at += 1;
        for index in 0..spu.numpal {
            let color = spu.masterpal[index];
            sector[at] = u8::try_from(color.y()).expect("Y is byte-ranged");
            sector[at + 1] = u8::try_from(color.cr()).expect("Cr is byte-ranged");
            sector[at + 2] = u8::try_from(color.cb()).expect("Cb is byte-ranged");
            at += 3;
        }
        if spu.numgroups != 0 {
            sector[at] = 2;
            at += 1;
            sector[at] = u8::try_from(spu.numgroups).expect("button groups are limited to three");
            at += 1;
            for group in 0..spu.numgroups {
                let mut words = [0u16; 4];
                for slot in (0..4).rev() {
                    let entry = spu.groupmap[group][slot];
                    if entry == -1 {
                        for word in &mut words {
                            *word <<= 4;
                        }
                    } else {
                        let highlight = spu.hlt.pal[((entry >> 8) & 0xFF) as usize];
                        let select = spu.sel.pal[(entry & 0xFF) as usize];
                        let highlight_index = spu.find_master_pal(highlight) as u16;
                        let select_index = spu.find_master_pal(select) as u16;
                        words[0] = (words[0] << 4) | highlight_index;
                        words[1] = (words[1] << 4) | u16::from(highlight.a >> 4);
                        words[2] = (words[2] << 4) | select_index;
                        words[3] = (words[3] << 4) | u16::from(select.a >> 4);
                    }
                }
                for word in words {
                    write_short(sector, &mut at, word);
                }
            }
        }
        if !spu.buttons.is_empty() {
            sector[at] = 3;
            at += 1;
            sector[at] = u8::try_from(spu.buttons.len()).expect("button count is limited to 36");
            at += 1;
            for (index, button) in spu.buttons.iter().enumerate() {
                write_string(sector, &mut at, button.name.as_deref().unwrap_or(""));
                write_short(sector, &mut at, 0);
                sector[at] = u8::from(button.autoaction);
                at += 1;
                sector[at] = u8::try_from(button.group).expect("button group is validated");
                at += 1;
                write_short(
                    sector,
                    &mut at,
                    u16::try_from(button.r.x0).expect("button x0 is validated"),
                );
                write_short(
                    sector,
                    &mut at,
                    u16::try_from(button.r.y0).expect("button y0 is validated"),
                );
                write_short(
                    sector,
                    &mut at,
                    u16::try_from(button.r.x1).expect("button x1 is validated"),
                );
                write_short(
                    sector,
                    &mut at,
                    u16::try_from(button.r.y1).expect("button y1 is validated"),
                );
                let first = if index == 0 { spu.buttons.len() } else { index };
                let second = if index + 1 == spu.buttons.len() {
                    1
                } else {
                    index + 2
                };
                let up = button.up.clone().unwrap_or_else(|| first.to_string());
                let down = button.down.clone().unwrap_or_else(|| second.to_string());
                let left = button.left.clone().unwrap_or_else(|| first.to_string());
                let right = button.right.clone().unwrap_or_else(|| second.to_string());
                write_string(sector, &mut at, &up);
                write_string(sector, &mut at, &down);
                write_string(sector, &mut at, &left);
                write_string(sector, &mut at, &right);
            }
        }
        let data = self.sector[..pdl].to_vec();
        self.write(&data);
    }

    fn write_subpicture(&mut self, spu: &SubPicture, sub: &[u8]) {
        let sub_size = sub.len();
        let mut sent = 0usize;
        let mut seq: u8 = 0;
        while sent != sub_size {
            if sent != 0 {
                self.header_size = 4;
            } else if self.header_size != 12 {
                self.header_size = 9;
            }
            let mut bytes_this_packet = SECTOR_SIZE - 20 - self.header_size;
            let mut stuffing = bytes_this_packet as i32 - (sub_size - sent) as i32;
            let stuffing = if stuffing < 0 {
                0
            } else {
                bytes_this_packet -= stuffing as usize;
                if stuffing > 7 {
                    stuffing = 0;
                }
                stuffing as usize
            };
            self.write_start(MPID_PACK);
            let mut header = [0u8; 32];
            pack_header(&mut header, self.lastgts, self.muxrate, 0);
            fix_gts(&mut self.lastgts, &mut self.nextgts);
            self.write(&header[..10]);
            self.write_start(MPID_PRIVATE1);
            let length = (bytes_this_packet + self.header_size + stuffing) as u16;
            self.write(&be16(length));
            if self.header_size == 9 {
                pes_header0(&mut header, spu.spts as u32);
            } else if self.header_size == 12 {
                pes_header1(&mut header, spu.spts as u32);
            } else {
                pes_header2(&mut header);
            }
            header[2] += stuffing as u8;
            for byte in header.iter_mut().skip(self.header_size - 1).take(stuffing) {
                *byte = 0xff;
            }
            header[self.header_size + stuffing - 1] = SUB_STREAM;
            self.write(&header[..self.header_size + stuffing]);
            seq = seq.wrapping_add(1);
            self.write(&sub[sent..sent + bytes_this_packet]);
            sent += bytes_this_packet;
            bytes_this_packet += 20 + self.header_size + stuffing;
            if bytes_this_packet != SECTOR_SIZE {
                self.write_start(MPID_PAD);
                let padding = (SECTOR_SIZE - bytes_this_packet - 6) as u16;
                self.write(&be16(padding));
                let fill = vec![0xff; padding as usize];
                self.write(&fill);
            }
            let _ = seq;
        }
    }

    fn run(&mut self, rgba: &mut RgbaReader<'_>) -> Result<(), String> {
        self.next = self.get_next_sub(rgba);
        let mut pending_pack = false;
        loop {
            if !pending_pack {
                self.mux_next(false, rgba)?;
                let mut code = [0u8; 4];
                if self.input.read_exact(&mut code).is_err() {
                    break;
                }
                let start = u32::from_be_bytes(code);
                if start == 0x100 + MPID_PACK {
                    pending_pack = true;
                } else if (0x100 + MPID_SYSTEM..=0x100 + MPID_VIDEO_LAST).contains(&start) {
                    self.write(&code);
                    let mut length = [0u8; 2];
                    if self.input.read_exact(&mut length).is_err() {
                        break;
                    }
                    self.write(&length);
                    let length = u16::from_be_bytes(length) as usize;
                    let mut payload = vec![0u8; length];
                    if self.input.read_exact(&mut payload).is_err() {
                        break;
                    }
                    if start == 0x100 + MPID_PRIVATE1
                        && private_substream(&payload)? == Some(SUB_STREAM)
                    {
                        return Err(format!("duplicate substream ID {SUB_STREAM:02x}"));
                    }
                    self.write(&payload);
                    if start == 0x100 + MPID_VIDEO_FIRST
                        && self.tofs == -1
                        && let Some(pts) = get_pts(&payload)
                    {
                        if let Some(next) = self.next.as_mut() {
                            next.spts += pts as i32;
                        }
                        self.tofs = pts as i32;
                    }
                } else if start == 0x100 + MPID_PROGRAM_END {
                    self.write(&code);
                } else {
                    self.write(&code);
                    let mut window = start;
                    loop {
                        let mut byte = [0u8; 1];
                        if self.input.read_exact(&mut byte).is_err() {
                            pending_pack = false;
                            break;
                        }
                        self.write(&byte);
                        window = (window << 8) | u32::from(byte[0]);
                        if window == 0x100 + MPID_PACK {
                            pending_pack = true;
                            break;
                        }
                    }
                    if !pending_pack {
                        break;
                    }
                }
            }
            if pending_pack {
                pending_pack = false;
                let mut psbuffer = [0u8; 10];
                if self.input.read_exact(&mut psbuffer).is_err() {
                    break;
                }
                if let Some(gts) = get_gts(&psbuffer) {
                    self.lastgts = gts;
                    self.mux_next(false, rgba)?;
                    fix_gts(&mut self.lastgts, &mut self.nextgts);
                    self.muxrate = get_muxrate(&psbuffer);
                } else {
                    self.lastgts = self.nextgts;
                }
                let mut header = [0u8; 32];
                pack_header(&mut header, self.lastgts, self.muxrate, 0);
                self.write_start(MPID_PACK);
                self.write(&header[..10]);
            }
        }
        self.mux_next(true, rgba)?;
        if self.skipped != 0 || self.subno + 1 != self.spus.len() as i32 {
            return Err("subpicture scheduling incomplete".into());
        }
        Ok(())
    }
}

pub(crate) fn run(
    document: Document,
    input: &Path,
    output: &Path,
    rgba: &mut RgbaReader<'_>,
) -> Result<(), String> {
    if document.spus.is_empty() {
        return Err("no subpictures".into());
    }
    if document.format != VideoFormat::Pal {
        // The historical menu path always programs PAL; keep the check explicit.
    }
    let input_file =
        std::fs::File::open(input).map_err(|error| format!("{}: {error}", input.display()))?;
    let mut mux = Mux::new(input_file, document);
    mux.run(rgba)?;
    let mut file =
        std::fs::File::create(output).map_err(|error| format!("{}: {error}", output.display()))?;
    file.write_all(&mux.output)
        .map_err(|error| format!("{}: {error}", output.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_pipeline_writes_subpicture_and_authoring_metadata() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("dvda-spu-pipeline-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let xml = directory.join("menu.xml");
        let input = directory.join("input.mpg");
        let output = directory.join("output.mpg");
        let image = directory.join("image.png");
        std::fs::write(
            &xml,
            format!(
                "<subpictures format=\"PAL\"><stream><spu force=\"yes\" start=\"00:00:00.00\" end=\"00:00:01.00\" image=\"{}\"/></stream></subpictures>",
                image.display()
            ),
        )
        .unwrap();

        let mut source = vec![0u8; SECTOR_SIZE];
        source[..4].copy_from_slice(&[0, 0, 1, MPID_PACK as u8]);
        let mut header = [0u8; 32];
        pack_header(&mut header, 90_000 * 300, 25_200, 0);
        source[4..14].copy_from_slice(&header[..10]);
        source[14..18].copy_from_slice(&[0, 0, 1, MPID_VIDEO_FIRST as u8]);
        source[18..20].copy_from_slice(&8u16.to_be_bytes());
        pes_header0(&mut header, 90_000);
        source[20..28].copy_from_slice(&header[..8]);
        source[28..32].copy_from_slice(&[0, 0, 1, MPID_PROGRAM_END as u8]);
        std::fs::write(&input, source).unwrap();

        let mut reads = 0;
        let mut rgba = |path: &Path, buffer: &mut [u8]| {
            assert_eq!(path, image);
            buffer[..16].copy_from_slice(&[255, 255, 255, 255].repeat(4));
            reads += 1;
            Ok((2, 2))
        };
        super::super::run(&xml, &input, &output, &mut rgba).unwrap();
        assert_eq!(reads, 1);

        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(bytes.len(), 3 * SECTOR_SIZE);
        let data = crate::nav::dvdata::parse_sector(&bytes[SECTOR_SIZE..2 * SECTOR_SIZE])
            .expect("private metadata sector")
            .unwrap();
        assert_eq!(data.start_pts, 90_000);
        assert_eq!(data.end_pts, 180_000);
        assert_eq!(
            &bytes[2 * SECTOR_SIZE + 14..2 * SECTOR_SIZE + 18],
            &[0, 0, 1, MPID_PRIVATE1 as u8]
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn truncated_pes_headers_are_rejected_without_panicking() {
        for payload in [
            &[][..],
            &[0][..],
            &[0, 0][..],
            &[0, 0, 0][..],
            &[0, 0, 4, 0][..],
        ] {
            assert!(private_substream(payload).is_err(), "payload {payload:?}");
        }
        assert_eq!(
            private_substream(&[0, 0, 0, SUB_STREAM]),
            Ok(Some(SUB_STREAM))
        );
        assert_eq!(private_substream(&[0, 0, 0, 0x80]), Ok(None));
        for length in 0..8 {
            assert_eq!(get_pts(&vec![0; length]), None);
        }
    }
}
