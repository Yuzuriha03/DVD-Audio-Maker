//! Streaming DVD-Audio pack/PES packetization for MLP and integer LPCM.
use crate::{
    Callbacks,
    audio::{self, AudioInfo},
    samg, timestamps,
};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct TrackInput {
    pub path: PathBuf,
    pub new_title: bool,
    pub cga: Option<u8>,
    pub downmix_rank: u8,
}
impl TrackInput {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            new_title: false,
            cga: None,
            downmix_rank: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct GroupOutput {
    pub tracks: Vec<samg::Track>,
    pub title_starts: Vec<usize>,
    pub aob_paths: Vec<PathBuf>,
    pub sectors: u32,
}
#[derive(Clone, Copy)]
struct Parameters {
    payload: usize,
    decrement: usize,
    first_header: u8,
    mid_header: u8,
    last_header: u8,
    mid_pes: u16,
}
impl Parameters {
    fn for_audio(info: &AudioInfo) -> Self {
        if info.mlp {
            return Self {
                payload: 2005,
                decrement: 21,
                first_header: 6,
                mid_header: 6,
                last_header: 6,
                mid_pes: 2028,
            };
        }
        const TABLE16: [[usize; 6]; 6] = [
            [2000, 16, 11, 16, 16, 2028],
            [2000, 16, 11, 16, 10, 2028],
            [2004, 24, 15, 12, 10, 2028],
            [2000, 16, 11, 16, 10, 2028],
            [2000, 20, 15, 16, 10, 2028],
            [1992, 24, 10, 10, 10, 2014],
        ];
        const TABLE24: [[usize; 6]; 6] = [
            [2004, 24, 15, 12, 12, 2028],
            [2004, 24, 15, 12, 10, 2028],
            [1998, 18, 15, 10, 10, 2020],
            [1992, 24, 10, 10, 10, 2014],
            [1980, 0, 15, 10, 10, 2002],
            [1980, 0, 15, 16, 16, 2008],
        ];
        let value = if info.bits == 16 {
            TABLE16[info.channels as usize - 1]
        } else if info.bits == 24 {
            TABLE24[info.channels as usize - 1]
        } else {
            // Twenty-bit sample pairs use five bytes/channel. All three packets
            // keep the same legal PES and sample-unit alignment as 16/24-bit.
            let unit = usize::from(info.channels) * 5;
            let payload = 2000 / unit * unit;
            let first_payload = 1984 / unit * unit;
            [payload, payload - first_payload, 11, 16, 10, payload + 28]
        };
        Self {
            payload: value[0],
            decrement: value[1],
            first_header: value[2] as u8,
            mid_header: value[3] as u8,
            last_header: value[4] as u8,
            mid_pes: value[5] as u16,
        }
    }
}

struct AudioReader {
    file: BufReader<File>,
    info: AudioInfo,
    remaining: u64,
    packed: Vec<u8>,
    position: usize,
    carry: Vec<u8>,
    carry_next: bool,
    finished: bool,
}
impl AudioReader {
    fn open(
        path: &Path,
        info: &AudioInfo,
        carry: Vec<u8>,
        carry_next: bool,
    ) -> Result<Self, String> {
        let mut file =
            BufReader::with_capacity(65536, File::open(path).map_err(|e| e.to_string())?);
        file.seek(SeekFrom::Start(info.data_offset))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            file,
            info: info.clone(),
            remaining: info.data_bytes,
            packed: vec![],
            position: 0,
            carry,
            carry_next,
            finished: false,
        })
    }
    fn refill(&mut self) -> Result<(), String> {
        self.packed.clear();
        self.position = 0;
        if self.finished {
            return Ok(());
        }
        if self.info.mlp {
            let length = self.remaining.min(65536) as usize;
            self.packed.resize(length, 0);
            self.file
                .read_exact(&mut self.packed)
                .map_err(|e| format!("Truncated MLP input: {e}"))?;
            self.remaining -= length as u64;
            self.finished = self.remaining == 0;
            return Ok(());
        }
        let container = usize::from(self.info.container_bits) / 8;
        let native = if self.info.bits == 16 { 2 } else { 3 };
        let align = usize::from(self.info.channels) * container;
        let request = (65536 / align * align).min(self.remaining as usize);
        let mut raw = vec![0; request];
        self.file
            .read_exact(&mut raw)
            .map_err(|e| format!("Truncated PCM input: {e}"))?;
        self.remaining -= request as u64;
        let mut canonical = std::mem::take(&mut self.carry);
        canonical.reserve(request);
        for word in raw.chunks_exact(container) {
            canonical.extend_from_slice(&word[container - native..]);
        }
        let pair = 2 * usize::from(self.info.channels) * native;
        if self.remaining == 0 {
            self.finished = true;
            if !self.carry_next && !canonical.len().is_multiple_of(pair) {
                canonical.resize(canonical.len().next_multiple_of(pair), 0);
            }
        }
        let full = canonical.len() / pair * pair;
        for samples in canonical[..full].chunks_exact(pair) {
            audio::append_pcm_pair(
                samples,
                self.info.bits,
                self.info.channels,
                &mut self.packed,
            );
        }
        self.carry.extend_from_slice(&canonical[full..]);
        Ok(())
    }
    fn read(&mut self, length: usize) -> Result<Vec<u8>, String> {
        let mut result = Vec::with_capacity(length);
        while result.len() < length {
            if self.position == self.packed.len() {
                self.refill()?;
                if self.packed.is_empty() {
                    break;
                }
            }
            let take = (length - result.len()).min(self.packed.len() - self.position);
            result.extend_from_slice(&self.packed[self.position..self.position + take]);
            self.position += take;
        }
        Ok(result)
    }
}

struct AobWriter {
    directory: PathBuf,
    group: u8,
    paths: Vec<PathBuf>,
    file: BufWriter<File>,
    sectors: u32,
    file_sectors: u32,
    split_sectors: u32,
}
impl AobWriter {
    fn new(directory: &Path, group: u8, split_sectors: u32) -> Result<Self, String> {
        if split_sectors == 0 || split_sectors > 524288 {
            return Err("AOB split size must be at most 1 GiB".into());
        }
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let path = directory.join(format!("ATS_{group:02}_1.AOB"));
        let file = BufWriter::with_capacity(65536, File::create(&path).map_err(|e| e.to_string())?);
        Ok(Self {
            directory: directory.into(),
            group,
            paths: vec![path],
            file,
            sectors: 0,
            file_sectors: 0,
            split_sectors,
        })
    }
    fn packet(&mut self, packet: &[u8]) -> Result<(), String> {
        if packet.len() != 2048 {
            return Err(format!(
                "AOB packet has {} bytes instead of 2048",
                packet.len()
            ));
        }
        if self.file_sectors == self.split_sectors {
            self.file.flush().map_err(|e| e.to_string())?;
            if self.paths.len() >= 9 {
                return Err("DVD-Audio titleset requires more than nine AOB files".into());
            }
            let path = self.directory.join(format!(
                "ATS_{:02}_{}.AOB",
                self.group,
                self.paths.len() + 1
            ));
            self.file =
                BufWriter::with_capacity(65536, File::create(&path).map_err(|e| e.to_string())?);
            self.paths.push(path);
            self.file_sectors = 0;
        }
        self.file.write_all(packet).map_err(|e| e.to_string())?;
        self.sectors = self
            .sectors
            .checked_add(1)
            .ok_or("AOB sector count overflow")?;
        self.file_sectors += 1;
        Ok(())
    }
}
fn padding(packet: &mut Vec<u8>) {
    let length = 2048 - packet.len();
    if length >= 6 {
        packet.extend_from_slice(&[0, 0, 1, 0xbe]);
        packet.extend_from_slice(&((length - 6) as u16).to_be_bytes());
        packet.resize(2048, 0xff);
    } else {
        packet.resize(2048, 0);
    }
}
fn clock(
    info: &AudioInfo,
    p: Parameters,
    index: u64,
    shift: u32,
) -> Result<(u32, u32, u64), String> {
    if info.mlp {
        let row = info.mlp_layout.get(index as usize);
        let (pts, dts, scr) = if let Some(row) = row {
            // Preserve C's division-before-multiplication rounding boundaries.
            let pts = ((row.samples + u64::from(info.frame_samples())) as f64
                / f64::from(info.rate)
                * 90000.0)
                .ceil() as u64
                + 23;
            let dts = (row.samples as f64 / f64::from(info.rate) * 90000.0).ceil() as u64
                + 23
                + u64::from(index == 0);
            let scr = if index == 0 {
                0
            } else {
                let time = (row.samples as f64 - f64::from(info.frame_samples()))
                    / f64::from(info.rate)
                    * 90000.0;
                let byteshift = row.offset as f64 - (1984.0 + index as f64 * 2005.0);
                ((time + 26.0 - byteshift / 14.0).round().max(0.0) as u64) * 300
            };
            (
                u32::try_from(pts).map_err(|_| "MLP PTS overflow")?,
                u32::try_from(dts).map_err(|_| "MLP DTS overflow")?,
                scr,
            )
        } else {
            (0, 0, 0)
        };
        Ok((
            pts.checked_add(shift).ok_or("PTS overflow")?,
            dts.checked_add(shift).ok_or("DTS overflow")?,
            scr + u64::from(shift) * 300,
        ))
    } else {
        let bytes = if index == 0 {
            0
        } else {
            index * p.payload as u64 - p.decrement as u64
        };
        let aligned = bytes / info.frame_bytes() * info.frame_bytes();
        let bps = u128::from(info.rate) * u128::from(info.channels) * u128::from(info.bits) / 8;
        let pts = u32::try_from((u128::from(aligned) * 90000 + bps / 2) / bps)
            .map_err(|_| "LPCM PTS overflow")?;
        let scr = (u128::from(bytes) * 27_000_000 / bps) as u64;
        Ok((
            pts.checked_add(shift).ok_or("PTS overflow")?,
            0,
            scr + u64::from(shift) * 300,
        ))
    }
}
fn packet(
    info: &AudioInfo,
    p: Parameters,
    index: u64,
    payload: &[u8],
    shift: u32,
    downmix: u8,
) -> Result<(Vec<u8>, u32), String> {
    let first = index == 0;
    let last = !first && payload.len() < p.payload;
    let header = if first {
        p.first_header
    } else if last {
        p.last_header
    } else {
        p.mid_header
    };
    let (pts, dts, scr) = clock(info, p, index, shift)?;
    let mut result = Vec::with_capacity(2048);
    result.extend_from_slice(&timestamps::pack_header(scr));
    if first {
        result.extend_from_slice(&[
            0, 0, 1, 0xbb, 0, 12, 0x80, 0xc4, 0xe1, 4, 0xa0, 0x7f, 0xb8, 0xc0, 0x40, 0xbd, 0xe0, 10,
        ]);
    }
    let pes_length = if info.mlp {
        payload.len() + if first { 26 } else { 23 }
    } else if first {
        15 + usize::from(header) + payload.len()
    } else if last {
        12 + usize::from(header) + payload.len()
    } else {
        usize::from(p.mid_pes)
    };
    result.extend_from_slice(&[0, 0, 1, 0xbd]);
    result.extend_from_slice(&(pes_length as u16).to_be_bytes());
    result.extend_from_slice(&[
        0x81,
        if info.mlp { 0xc0 } else { 0x80 } | u8::from(first),
        if info.mlp { 10 } else { 5 } + if first { 3 } else { 0 },
    ]);
    if info.mlp {
        // C's zero-filled schedule beyond the final AU deliberately omits the
        // timestamp bytes. Ordinary final packets have a nonzero final row.
        if dts != 0 {
            result.extend_from_slice(&timestamps::pack_timestamp(pts, 0x30));
            result.extend_from_slice(&timestamps::pack_timestamp(dts, 0x10));
        }
    } else {
        result.extend_from_slice(&timestamps::pack_timestamp(pts, 0x20));
    }
    if first {
        result.extend_from_slice(&[0x1e, 0x60, 10]);
    }
    let pointer = if info.mlp {
        if last {
            0
        } else {
            let offset = if index == 0 {
                0
            } else {
                (index - 1) * 2005 + 1984
            };
            info.mlp_layout
                .get(index as usize)
                .map_or(0, |row| row.offset.saturating_sub(offset))
                + u64::from(header)
                - 1
        }
    } else {
        let bytes = if index == 0 {
            0
        } else {
            index * p.payload as u64 - p.decrement as u64
        };
        bytes.div_ceil(info.frame_bytes()) * info.frame_bytes() - bytes + u64::from(header) - 1
    };
    let pointer = u16::try_from(pointer)
        .map_err(|_| "First access-unit pointer exceeds DVD header capacity")?;
    result.extend_from_slice(&[
        if info.mlp { 0xa1 } else { 0xa0 },
        (index % 32) as u8,
        0,
        header,
    ]);
    result.extend_from_slice(&pointer.to_be_bytes());
    if info.mlp {
        result.extend_from_slice(&[0, 0, 0, 0]);
    } else {
        let size = match info.bits {
            16 => 0,
            20 => 1,
            _ => 2,
        };
        let rate = match info.rate {
            48000 => 0,
            96000 => 1,
            192000 => 2,
            44100 => 8,
            88200 => 9,
            _ => 10,
        };
        result.extend_from_slice(&[
            if info.channels < 3 {
                0x10
            } else {
                downmix.saturating_sub(1)
            },
            if info.channels < 3 {
                (size << 4) | 15
            } else {
                size * 17
            },
            if info.channels < 3 {
                (rate << 4) | 15
            } else {
                rate * 17
            },
            0,
            info.channel_assignment,
            0x80,
        ]);
        result.resize(result.len() + usize::from(header) - 8, 0);
    }
    result.extend_from_slice(payload);
    if result.len() > 2048 {
        return Err("Audio PES payload exceeds one sector".into());
    }
    padding(&mut result);
    Ok((result, pts))
}

pub fn author_group(
    audio_ts: &Path,
    group: u8,
    inputs: &[TrackInput],
    callbacks: &mut dyn Callbacks,
) -> Result<GroupOutput, String> {
    author_group_with_split(audio_ts, group, inputs, callbacks, 524288)
}
/// A smaller split limit supports acceptance without generating a GiB fixture.
pub fn author_group_with_split(
    audio_ts: &Path,
    group: u8,
    inputs: &[TrackInput],
    callbacks: &mut dyn Callbacks,
    split_sectors: u32,
) -> Result<GroupOutput, String> {
    if !(1..=9).contains(&group) || inputs.is_empty() || inputs.len() > 99 {
        return Err("An audio group requires 1..99 tracks and a group number 1..9".into());
    }
    let mut infos = Vec::with_capacity(inputs.len());
    for track in inputs {
        if callbacks.cancelled() {
            return Err("Authoring cancelled".into());
        }
        if track.downmix_rank > 16 {
            return Err("Downmix rank must be 0..16".into());
        }
        let mut info = audio::probe_with_callbacks(&track.path, callbacks)?;
        if let Some(cga) = track.cga {
            if audio::CHANNELS.get(cga as usize) != Some(&info.channels) {
                return Err("CGA does not match the input channel count".into());
            }
            if info.mlp && cga != info.channel_assignment {
                return Err("MLP channel assignment cannot be changed without re-encoding".into());
            }
            info.channel_assignment = cga;
        }
        infos.push(info);
    }
    let mut writer = AobWriter::new(audio_ts, group, split_sectors)?;
    let mut tracks = vec![];
    let mut title_starts = vec![];
    let mut shift = 0u32;
    let mut carry = vec![];
    let total: u64 = infos.iter().map(|info| info.data_bytes).sum();
    let mut completed = 0u64;
    for (index, (track, info)) in inputs.iter().zip(&infos).enumerate() {
        let new_title = index == 0 || track.new_title || !info.same_format(&infos[index - 1]);
        if new_title {
            title_starts.push(index);
            shift = 0;
            carry.clear();
        }
        let carry_next = !info.mlp
            && index + 1 < inputs.len()
            && !inputs[index + 1].new_title
            && info.same_format(&infos[index + 1]);
        let mut reader =
            AudioReader::open(&track.path, info, std::mem::take(&mut carry), carry_next)?;
        let parameters = Parameters::for_audio(info);
        let mut pack_index = 0u64;
        let first_sector = writer.sectors;
        let mut first_pts = None;
        callbacks.emit(
            1,
            &format!("[INF] Rust author processing {}\n", track.path.display()),
        );
        loop {
            if callbacks.cancelled() {
                return Err("Authoring cancelled".into());
            }
            let capacity = if pack_index == 0 {
                parameters.payload - parameters.decrement
            } else {
                parameters.payload
            };
            let payload = reader.read(capacity)?;
            if payload.is_empty() && pack_index != 0 && index + 1 == inputs.len() {
                break;
            }
            if payload.is_empty() && pack_index == 0 && info.mlp {
                return Err("Empty MLP stream".into());
            }
            let short = payload.len() < capacity;
            let (bytes, pts) = packet(
                info,
                parameters,
                pack_index,
                &payload,
                shift,
                track.downmix_rank,
            )?;
            first_pts.get_or_insert(pts);
            writer.packet(&bytes)?;
            pack_index += 1;
            if pack_index.is_multiple_of(128) || short {
                callbacks.progress(completed + info.data_bytes - reader.remaining, total);
            }
            if short {
                break;
            }
        }
        carry = std::mem::take(&mut reader.carry);
        let pts_length = info.pts_length()?;
        tracks.push(samg::Track {
            mlp: info.mlp,
            bits: info.bits,
            rate: info.rate,
            channels: info.channels,
            channel_assignment: info.channel_assignment,
            first_pts: first_pts.unwrap_or(shift),
            pts_length,
            first_sector,
            last_sector: writer.sectors - 1,
        });
        shift = shift
            .checked_add(pts_length)
            .ok_or("Title PTS duration overflow")?;
        completed += info.data_bytes;
        callbacks.progress(completed, total);
    }
    writer.file.flush().map_err(|e| e.to_string())?;
    Ok(GroupOutput {
        tracks,
        title_starts,
        aob_paths: writer.paths,
        sectors: writer.sectors,
    })
}
