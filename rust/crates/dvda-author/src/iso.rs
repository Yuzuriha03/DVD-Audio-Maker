//! Streaming ISO9660 / UDF 1.02 bridge writer, migrated from iso_writer.c.
//! Directory records are alphabetical; AUDIO_TS payloads follow DVD-Audio order.
use std::{
    cmp::Ordering,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering as AtomicOrdering},
};

const SECTOR: u32 = 2048;
const PARTITION: u32 = 257;
const DIRECTORY_START: u32 = 259;
const MAIN_VDS: u32 = 32;
const RESERVE_VDS: u32 = 48;
const INTEGRITY: u32 = 64;
const ANCHOR: u32 = 256;
type Sector = [u8; SECTOR as usize];

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn sectors(bytes: u32) -> u32 {
    bytes.div_ceil(SECTOR)
}
fn advance(next: &mut u32, count: u32) -> io::Result<u32> {
    let start = *next;
    *next = next
        .checked_add(count)
        .ok_or_else(|| invalid("ISO sector overflow"))?;
    Ok(start)
}
fn le16(data: &mut [u8], at: usize, value: u16) {
    data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn le32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn le64(data: &mut [u8], at: usize, value: u64) {
    data[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn both16(data: &mut [u8], at: usize, value: u16) {
    le16(data, at, value);
    data[at + 2..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn both32(data: &mut [u8], at: usize, value: u32) {
    le32(data, at, value);
    data[at + 4..at + 8].copy_from_slice(&value.to_be_bytes());
}
fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for byte in data {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x1021 } else { 0 };
        }
    }
    crc
}
fn tag(data: &mut [u8], id: u16, location: u32, length: u16) {
    le16(data, 0, id);
    le16(data, 2, 2);
    data[4] = 0;
    le16(data, 6, 1);
    le16(data, 8, crc16(&data[16..16 + usize::from(length)]));
    le16(data, 10, length);
    le32(data, 12, location);
    data[4] = data[..16]
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
}
fn entity(data: &mut [u8], at: usize, name: &[u8], suffix: Option<[u8; 3]>) {
    let length = name.len().min(23);
    data[at + 1..at + 1 + length].copy_from_slice(&name[..length]);
    if let Some(suffix) = suffix {
        data[at + 24..at + 27].copy_from_slice(&suffix);
    }
}
fn charspec(data: &mut [u8], at: usize) {
    data[at + 1..at + 24].copy_from_slice(b"OSTA Compressed Unicode");
}
fn timestamp(data: &mut [u8], at: usize) {
    le16(data, at, 0x1000);
    le16(data, at + 2, 2000);
    data[at + 4] = 1;
    data[at + 5] = 1;
}
fn extent(data: &mut [u8], at: usize, location: u32, length: u32) {
    le32(data, at, length);
    le32(data, at + 4, location);
}
fn osta_name(name: &str) -> Vec<u8> {
    if name.chars().all(|c| u32::from(c) <= 255) {
        let mut bytes = vec![8];
        bytes.extend(name.chars().map(|c| c as u8));
        bytes
    } else {
        let mut bytes = vec![16];
        for unit in name.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        bytes
    }
}
fn dstring(data: &mut [u8], at: usize, size: usize, text: &str) {
    // Recompute compression when adding a character, as in the C writer.
    let mut end = 0;
    for (offset, c) in text.char_indices() {
        let candidate = offset + c.len_utf8();
        if osta_name(&text[..candidate]).len() > (size - 1).min(255) {
            break;
        }
        end = candidate;
    }
    let bytes = osta_name(&text[..end]);
    data[at..at + bytes.len()].copy_from_slice(&bytes);
    data[at + size - 1] = bytes.len() as u8;
}
fn padded(data: &mut [u8], at: usize, size: usize, value: &[u8]) {
    data[at..at + size].fill(b' ');
    let length = value.len().min(size);
    data[at..at + length].copy_from_slice(&value[..length]);
}
fn zeroes(output: &mut impl Write, mut count: u64) -> io::Result<()> {
    let zero = [0; SECTOR as usize];
    while count > 0 {
        let length = count.min(u64::from(SECTOR)) as usize;
        output.write_all(&zero[..length])?;
        count -= length as u64;
    }
    Ok(())
}

#[derive(Debug)]
struct Node {
    name: String,
    path: PathBuf,
    parent: usize,
    file: bool,
    children: Vec<usize>,
    size: u32,
    lba: u32,
    path_index: u16,
    udf_lba: u32,
    udf_length: u32,
    udf_sectors: u32,
    entry_lba: u32,
}
struct Image {
    nodes: Vec<Node>,
    directories: Vec<usize>,
    path_directories: Vec<usize>,
    files: Vec<usize>,
    entries: Vec<usize>,
    path_size: u32,
    path_sectors: u32,
    path_lba: u32,
    path_m_lba: u32,
    end_anchor: u32,
}
fn compare_names(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|b| b.to_ascii_lowercase())
        .cmp(b.bytes().map(|b| b.to_ascii_lowercase()))
}
fn dvd_rank(node: &Node, nodes: &[Node]) -> Option<u32> {
    if !node.file
        || node.parent == 0
        || nodes[node.parent].parent != 0
        || !nodes[node.parent].name.eq_ignore_ascii_case("AUDIO_TS")
        || node.name.len() != 12
    {
        return None;
    }
    let name = node.name.to_ascii_uppercase();
    let bytes = name.as_bytes();
    for (prefix, value) in [b"AUDIO_PP", b"AUDIO_TS", b"AUDIO_SV"].iter().enumerate() {
        if &bytes[..8] == *value {
            for (extension, value) in [b".IFO", b".VOB", b".BUP"].iter().enumerate() {
                if &bytes[8..] == *value {
                    return Some((prefix * 3 + extension) as u32);
                }
            }
        }
    }
    if &bytes[..4] != b"ATS_"
        || !bytes[4..6].iter().all(u8::is_ascii_digit)
        || bytes[6] != b'_'
        || !bytes[7].is_ascii_digit()
    {
        return None;
    }
    let title = u32::from(bytes[4] - b'0') * 10 + u32::from(bytes[5] - b'0');
    let extension = [b".IFO", b".AOB", b".BUP"]
        .iter()
        .position(|value| &bytes[8..] == *value)?;
    (title > 0).then_some(9 + title * 30 + extension as u32 * 10 + u32::from(bytes[7] - b'0'))
}
fn scan(
    nodes: &mut Vec<Node>,
    path: &Path,
    name: String,
    parent: usize,
    depth: usize,
) -> io::Result<usize> {
    if depth > 128 {
        return Err(invalid("ISO directory nesting exceeds 128"));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        return Err(invalid(
            "ISO input must contain regular files and directories",
        ));
    }
    let file = metadata.is_file();
    if depth == 0 && file {
        return Err(invalid("ISO source must be a directory"));
    }
    if depth > 0 && (name.len() > if file { 219 } else { 31 } || osta_name(&name).len() > 255) {
        return Err(invalid("Filename exceeds ISO9660/UDF record capacity"));
    }
    let size = if file {
        u32::try_from(metadata.len()).map_err(|_| invalid("ISO input file exceeds 4 GiB - 1"))?
    } else {
        0
    };
    let id = nodes.len();
    nodes.push(Node {
        name,
        path: path.into(),
        parent,
        file,
        children: Vec::new(),
        size,
        lba: 0,
        path_index: 0,
        udf_lba: 0,
        udf_length: 0,
        udf_sectors: 0,
        entry_lba: 0,
    });
    if !file {
        let mut children = fs::read_dir(path)?
            .map(|item| {
                let item = item?;
                let name = item
                    .file_name()
                    .into_string()
                    .map_err(|_| invalid("ISO filename is not Unicode"))?;
                Ok((name, item.path()))
            })
            .collect::<io::Result<Vec<_>>>()?;
        children.sort_by(|a, b| compare_names(&a.0, &b.0));
        if children
            .windows(2)
            .any(|pair| compare_names(&pair[0].0, &pair[1].0) == Ordering::Equal)
        {
            return Err(invalid("ISO filenames collide ignoring ASCII case"));
        }
        for (name, path) in children {
            let child = scan(nodes, &path, name, id, depth + 1)?;
            nodes[id].children.push(child);
        }
    }
    Ok(id)
}
fn iso_record_length(node: &Node) -> u32 {
    let length = node.name.len() as u32 + if node.file { 2 } else { 0 };
    33 + length + u32::from(length.is_multiple_of(2))
}
fn aligned_record(offset: u32, length: u32) -> io::Result<u32> {
    let padding = if offset % SECTOR + length > SECTOR {
        SECTOR - offset % SECTOR
    } else {
        0
    };
    offset
        .checked_add(padding)
        .and_then(|value| value.checked_add(length))
        .ok_or_else(|| invalid("ISO directory size overflow"))
}
impl Image {
    fn plan(source: &Path) -> io::Result<Self> {
        let mut nodes = Vec::new();
        scan(&mut nodes, source, String::new(), 0, 0)?;
        let directories: Vec<_> = (0..nodes.len()).filter(|id| !nodes[*id].file).collect();
        // ISO9660 path tables order directories by depth, then parent index,
        // then name. Keep data extents in the original author's DFS order.
        let mut path_directories = vec![0];
        let mut cursor = 0;
        while cursor < path_directories.len() {
            let id = path_directories[cursor];
            path_directories.extend(
                nodes[id]
                    .children
                    .iter()
                    .copied()
                    .filter(|child| !nodes[*child].file),
            );
            cursor += 1;
        }
        for (index, id) in path_directories.iter().copied().enumerate() {
            nodes[id].path_index =
                u16::try_from(index + 1).map_err(|_| invalid("Too many ISO directories"))?;
        }
        let mut path_size = 0u32;
        let mut next = DIRECTORY_START;
        for id in directories.iter().copied() {
            let length = if id == 0 {
                1
            } else {
                nodes[id].name.len() as u32
            };
            advance(&mut path_size, 8 + length + length % 2)?;
            let mut udf_length = 40;
            let mut iso_length = 68;
            for child in &nodes[id].children {
                let fid = (38 + osta_name(&nodes[*child].name).len() as u32).next_multiple_of(4);
                // ECMA-167 directory streams contain consecutive FIDs. FIDs
                // may straddle blocks; only each record's four-byte pad remains.
                advance(&mut udf_length, fid)?;
                iso_length = aligned_record(iso_length, iso_record_length(&nodes[*child]))?;
            }
            let node = &mut nodes[id];
            node.udf_length = udf_length;
            node.udf_sectors = sectors(udf_length);
            node.udf_lba = advance(&mut next, node.udf_sectors)?;
            node.size = sectors(iso_length)
                .checked_mul(SECTOR)
                .ok_or_else(|| invalid("ISO directory overflow"))?;
        }
        // Arena insertion follows the original author's recursive entry order.
        let entries: Vec<_> = (0..nodes.len()).collect();
        for node in &mut nodes {
            node.entry_lba = advance(&mut next, 1)?;
        }
        let path_sectors = sectors(path_size);
        if path_sectors > 0x7fff {
            return Err(invalid("ISO path table exceeds capacity"));
        }
        let path_lba = advance(&mut next, path_sectors)?;
        let path_m_lba = advance(&mut next, path_sectors)?;
        for id in &directories {
            nodes[*id].lba = advance(&mut next, sectors(nodes[*id].size))?;
        }
        let mut files = Vec::new();
        for id in &directories {
            let mut children: Vec<_> = nodes[*id]
                .children
                .iter()
                .copied()
                .filter(|child| nodes[*child].file)
                .collect();
            children.sort_by(|a, b| {
                match (dvd_rank(&nodes[*a], &nodes), dvd_rank(&nodes[*b], &nodes)) {
                    (Some(a), Some(b)) if a != b => a.cmp(&b),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    _ => compare_names(&nodes[*a].name, &nodes[*b].name),
                }
            });
            for child in children {
                nodes[child].lba = advance(&mut next, sectors(nodes[child].size))?;
                files.push(child);
            }
        }
        next.checked_add(1)
            .ok_or_else(|| invalid("ISO volume overflow"))?;
        Ok(Self {
            nodes,
            directories,
            path_directories,
            files,
            entries,
            path_size,
            path_sectors,
            path_lba,
            path_m_lba,
            end_anchor: next,
        })
    }
    fn iso_record(&self, id: usize, special: Option<u8>) -> Vec<u8> {
        let node = &self.nodes[id];
        let name = match special {
            Some(value) => vec![value],
            None => {
                let mut name = node.name.as_bytes().to_vec();
                if node.file {
                    name.extend_from_slice(b";1");
                }
                name
            }
        };
        let length = 33 + name.len() + usize::from(name.len().is_multiple_of(2));
        let mut record = vec![0; length];
        record[0] = length as u8;
        both32(&mut record, 2, node.lba);
        both32(&mut record, 10, node.size);
        record[25] = if node.file { 0 } else { 2 };
        both16(&mut record, 28, 1);
        record[32] = name.len() as u8;
        record[33..33 + name.len()].copy_from_slice(&name);
        record
    }
    fn fid(&self, directory: usize, target: usize, parent: bool, offset: u32) -> Vec<u8> {
        let node = &self.nodes[target];
        let name = if parent {
            Vec::new()
        } else {
            osta_name(&node.name)
        };
        let length = (38 + name.len()).next_multiple_of(4);
        let mut record = vec![0; length];
        le16(&mut record, 16, 1);
        record[18] = if node.file { 0 } else { 2 } | if parent { 8 } else { 0 };
        record[19] = name.len() as u8;
        extent(&mut record, 20, node.entry_lba - PARTITION, SECTOR);
        record[38..38 + name.len()].copy_from_slice(&name);
        tag(
            &mut record,
            257,
            self.nodes[directory].udf_lba - PARTITION + offset / SECTOR,
            (length - 16) as u16,
        );
        record
    }
    fn file_entry(&self, id: usize) -> Sector {
        let node = &self.nodes[id];
        let mut data = [0; SECTOR as usize];
        le16(&mut data, 20, 4);
        le16(&mut data, 24, 1);
        data[27] = if node.file { 5 } else { 4 };
        le16(&mut data, 34, 0x0230);
        le32(&mut data, 36, u32::MAX);
        le32(&mut data, 40, u32::MAX);
        le32(&mut data, 44, 0x111);
        le16(&mut data, 48, 1);
        le64(
            &mut data,
            56,
            u64::from(if node.file {
                node.size
            } else {
                node.udf_length
            }),
        );
        le64(
            &mut data,
            64,
            u64::from(if node.file {
                sectors(node.size)
            } else {
                node.udf_sectors
            }),
        );
        for at in [72, 84, 96] {
            timestamp(&mut data, at);
        }
        le32(&mut data, 108, 1);
        entity(&mut data, 128, b"*DVD-Audio Maker", None);
        le64(&mut data, 160, u64::from(node.entry_lba - PARTITION + 1));
        let mut allocation_bytes = 0;
        if node.file {
            let mut remaining = node.size;
            let mut location = node.lba - PARTITION;
            while remaining > 0 {
                let length = remaining.min(0x3ffff800);
                extent(&mut data, 176 + allocation_bytes, location, length);
                allocation_bytes += 8;
                location += sectors(length);
                remaining -= length;
            }
        } else {
            extent(&mut data, 176, node.udf_lba - PARTITION, node.udf_length);
            allocation_bytes = 8;
        }
        le32(&mut data, 172, allocation_bytes as u32);
        tag(
            &mut data,
            261,
            node.entry_lba - PARTITION,
            (160 + allocation_bytes) as u16,
        );
        data
    }
    fn write(&self, output: &mut impl Write, label: &str) -> io::Result<()> {
        zeroes(output, 16 * u64::from(SECTOR))?;
        let mut pvd = [0; SECTOR as usize];
        pvd[..7].copy_from_slice(b"\x01CD001\x01");
        padded(&mut pvd, 8, 32, b"DVDA-MAKER");
        padded(&mut pvd, 40, 32, label.as_bytes());
        both32(&mut pvd, 80, self.end_anchor + 1);
        for at in [120, 124] {
            both16(&mut pvd, at, 1);
        }
        both16(&mut pvd, 128, SECTOR as u16);
        both32(&mut pvd, 132, self.path_size);
        le32(&mut pvd, 140, self.path_lba);
        pvd[148..152].copy_from_slice(&self.path_m_lba.to_be_bytes());
        pvd[156..190].copy_from_slice(&self.iso_record(0, Some(0)));
        pvd[881] = 1;
        output.write_all(&pvd)?;
        let mut end = [0; SECTOR as usize];
        end[..7].copy_from_slice(b"\xffCD001\x01");
        output.write_all(&end)?;
        for name in [b"BEA01", b"NSR02", b"TEA01"] {
            let mut data = [0; SECTOR as usize];
            data[1..6].copy_from_slice(name);
            data[6] = 1;
            output.write_all(&data)?;
        }
        zeroes(output, 11 * u64::from(SECTOR))?;
        self.write_vds(output, label)?;
        zeroes(
            output,
            u64::from(ANCHOR - INTEGRITY - 2) * u64::from(SECTOR),
        )?;
        output.write_all(&anchor(ANCHOR))?;
        let mut data = [0; SECTOR as usize];
        timestamp(&mut data, 16);
        le16(&mut data, 28, 3);
        le16(&mut data, 30, 3);
        le32(&mut data, 32, 1);
        le32(&mut data, 36, 1);
        charspec(&mut data, 48);
        dstring(&mut data, 112, 128, label);
        charspec(&mut data, 240);
        dstring(&mut data, 304, 32, label);
        extent(&mut data, 400, self.nodes[0].entry_lba - PARTITION, SECTOR);
        entity(&mut data, 416, b"*OSTA UDF Compliant", Some([2, 1, 3]));
        write_descriptor(output, data, 256, 0, 496)?;
        write_descriptor(output, [0; SECTOR as usize], 8, 1, 496)?;
        for id in &self.directories {
            let node = &self.nodes[*id];
            let mut offset = 0;
            for (target, parent) in std::iter::once((node.parent, true))
                .chain(node.children.iter().map(|id| (*id, false)))
            {
                let length = if parent {
                    40
                } else {
                    (38 + osta_name(&self.nodes[target].name).len() as u32).next_multiple_of(4)
                };
                output.write_all(&self.fid(*id, target, parent, offset))?;
                advance(&mut offset, length)?;
            }
            zeroes(
                output,
                u64::from(node.udf_sectors) * u64::from(SECTOR) - u64::from(offset),
            )?;
        }
        for id in &self.entries {
            output.write_all(&self.file_entry(*id))?;
        }
        for big in [false, true] {
            for id in &self.path_directories {
                let node = &self.nodes[*id];
                let name = if *id == 0 {
                    &[0][..]
                } else {
                    node.name.as_bytes()
                };
                let mut entry = vec![0; 8 + name.len() + name.len() % 2];
                entry[0] = name.len() as u8;
                let parent = self.nodes[node.parent].path_index;
                if big {
                    entry[2..6].copy_from_slice(&node.lba.to_be_bytes());
                    entry[6..8].copy_from_slice(&parent.to_be_bytes());
                } else {
                    le32(&mut entry, 2, node.lba);
                    le16(&mut entry, 6, parent);
                }
                entry[8..8 + name.len()].copy_from_slice(name);
                output.write_all(&entry)?;
            }
            zeroes(
                output,
                u64::from(self.path_sectors) * u64::from(SECTOR) - u64::from(self.path_size),
            )?;
        }
        for id in &self.directories {
            let node = &self.nodes[*id];
            output.write_all(&self.iso_record(*id, Some(0)))?;
            output.write_all(&self.iso_record(node.parent, Some(1)))?;
            let mut offset = 68;
            for child in &node.children {
                let record = self.iso_record(*child, None);
                let next = aligned_record(offset, record.len() as u32)?;
                zeroes(output, u64::from(next - offset - record.len() as u32))?;
                output.write_all(&record)?;
                offset = next;
            }
            zeroes(output, u64::from(node.size - offset))?;
        }
        let mut buffer = [0; 64 * 1024];
        for id in &self.files {
            let node = &self.nodes[*id];
            let mut input = File::open(&node.path)?;
            let mut remaining = u64::from(node.size);
            while remaining > 0 {
                let capacity = buffer.len().min(remaining as usize);
                let length = input.read(&mut buffer[..capacity])?;
                if length == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "ISO input shrank while writing",
                    ));
                }
                output.write_all(&buffer[..length])?;
                remaining -= length as u64;
            }
            if input.read(&mut buffer[..1])? != 0 {
                return Err(invalid("ISO input grew while writing"));
            }
            zeroes(
                output,
                u64::from(sectors(node.size)) * u64::from(SECTOR) - u64::from(node.size),
            )?;
        }
        output.write_all(&anchor(self.end_anchor))
    }
    fn write_vds(&self, output: &mut impl Write, label: &str) -> io::Result<()> {
        let partition_length = self.end_anchor - PARTITION;
        for base in [MAIN_VDS, RESERVE_VDS] {
            let mut data = [0; SECTOR as usize];
            dstring(&mut data, 24, 32, label);
            for (at, value) in [(56, 1), (58, 1), (60, 2), (62, 2)] {
                le16(&mut data, at, value);
            }
            le32(&mut data, 64, 1);
            le32(&mut data, 68, 1);
            dstring(&mut data, 72, 128, label);
            charspec(&mut data, 200);
            charspec(&mut data, 264);
            timestamp(&mut data, 376);
            entity(&mut data, 388, b"*DVD-Audio Maker", None);
            write_descriptor(output, data, 1, base, 496)?;
            let mut data = [0; SECTOR as usize];
            le32(&mut data, 16, 1);
            entity(&mut data, 20, b"*UDF LV Info", Some([2, 1, 0]));
            charspec(&mut data, 52);
            dstring(&mut data, 116, 128, label);
            write_descriptor(output, data, 4, base + 1, 496)?;
            let mut data = [0; SECTOR as usize];
            le32(&mut data, 16, 2);
            le16(&mut data, 20, 1);
            entity(&mut data, 24, b"+NSR02", None);
            data[24] = 2;
            le32(&mut data, 184, 1);
            le32(&mut data, 188, PARTITION);
            le32(&mut data, 192, partition_length);
            entity(&mut data, 196, b"*DVD-Audio Maker", None);
            write_descriptor(output, data, 5, base + 2, 496)?;
            let mut data = [0; SECTOR as usize];
            le32(&mut data, 16, 3);
            charspec(&mut data, 20);
            dstring(&mut data, 84, 128, label);
            le32(&mut data, 212, SECTOR);
            entity(&mut data, 216, b"*OSTA UDF Compliant", Some([2, 1, 3]));
            extent(&mut data, 248, 0, 2 * SECTOR);
            le32(&mut data, 264, 6);
            le32(&mut data, 268, 1);
            entity(&mut data, 272, b"*DVD-Audio Maker", None);
            extent(&mut data, 432, INTEGRITY, 2 * SECTOR);
            data[440] = 1;
            data[441] = 6;
            le16(&mut data, 442, 1);
            write_descriptor(output, data, 6, base + 3, 430)?;
            let mut data = [0; SECTOR as usize];
            le32(&mut data, 16, 4);
            write_descriptor(output, data, 7, base + 4, 8)?;
            write_descriptor(output, [0; SECTOR as usize], 8, base + 5, 496)?;
            zeroes(output, 10 * u64::from(SECTOR))?;
        }
        let mut data = [0; SECTOR as usize];
        timestamp(&mut data, 16);
        le32(&mut data, 28, 1);
        le64(&mut data, 40, u64::from(partition_length + 1));
        le32(&mut data, 72, 1);
        le32(&mut data, 76, 46);
        le32(&mut data, 84, partition_length);
        entity(&mut data, 88, b"*DVD-Audio Maker", None);
        le32(&mut data, 120, self.files.len() as u32);
        le32(&mut data, 124, self.directories.len() as u32);
        for at in [128, 130, 132] {
            le16(&mut data, at, 0x102);
        }
        write_descriptor(output, data, 9, INTEGRITY, 118)?;
        write_descriptor(output, [0; SECTOR as usize], 8, INTEGRITY + 1, 496)
    }
}
fn write_descriptor(
    output: &mut impl Write,
    mut data: Sector,
    id: u16,
    location: u32,
    length: u16,
) -> io::Result<()> {
    tag(&mut data, id, location, length);
    output.write_all(&data)
}
fn anchor(location: u32) -> Sector {
    let mut data = [0; SECTOR as usize];
    extent(&mut data, 16, MAIN_VDS, 16 * SECTOR);
    extent(&mut data, 24, RESERVE_VDS, 16 * SECTOR);
    tag(&mut data, 2, location, 496);
    data
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
/// Write a disc image with bounded memory and atomically replace the destination
/// only after every source file, buffered write and final flush succeeds.
pub fn write(source: &Path, destination: &Path, label: Option<&str>) -> io::Result<()> {
    write_with_callbacks(source, destination, label, &mut crate::Silent)
}

/// File payload addresses from the same layout used by the writer. This lets
/// SAMG reference the actual AOB locations, including filesystem metadata.
pub fn file_layout(source: &Path) -> io::Result<Vec<(PathBuf, u32, u32)>> {
    let source = source.canonicalize()?;
    let image = Image::plan(&source)?;
    Ok(image
        .files
        .iter()
        .map(|&id| {
            let node = &image.nodes[id];
            (
                node.path.strip_prefix(&source).unwrap().to_owned(),
                node.lba,
                node.size,
            )
        })
        .collect())
}

pub fn write_with_callbacks(
    source: &Path,
    destination: &Path,
    label: Option<&str>,
    callbacks: &mut dyn crate::Callbacks,
) -> io::Result<()> {
    let source = source.canonicalize()?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    if parent.starts_with(&source) {
        return Err(invalid("ISO destination must be outside the source tree"));
    }
    let filename = destination
        .file_name()
        .ok_or_else(|| invalid("ISO destination requires a filename"))?;
    let destination = parent.join(filename);
    let image = Image::plan(&source)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (temporary, file) = loop {
        let path = parent.join(format!(
            ".dvda-iso-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (Temporary(path), file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    {
        let mut output = BufWriter::with_capacity(64 * 1024, file);
        struct Progress<'a, W> {
            output: &'a mut W,
            callbacks: &'a mut dyn crate::Callbacks,
            completed: u64,
            total: u64,
        }
        impl<W: Write> Write for Progress<'_, W> {
            fn write(&mut self, data: &[u8]) -> io::Result<usize> {
                if self.callbacks.cancelled() {
                    // write_all retries Interrupted, so cancellation must be
                    // a terminal error rather than an interrupted syscall.
                    return Err(io::Error::other("Author cancelled"));
                }
                let size = self.output.write(data)?;
                self.completed += size as u64;
                self.callbacks.progress(self.completed, self.total);
                Ok(size)
            }
            fn flush(&mut self) -> io::Result<()> {
                self.output.flush()
            }
        }
        image.write(
            &mut Progress {
                output: &mut output,
                callbacks,
                completed: 0,
                total: (u64::from(image.end_anchor) + 1) * u64::from(SECTOR),
            },
            label.unwrap_or("DVD-AUDIO"),
        )?;
        output.flush()?;
        output.get_ref().sync_all()?;
    }
    fs::rename(&temporary.0, &destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_and_unicode_encoding() {
        assert_eq!(crc16(b"123456789"), 0x31c3);
        assert_eq!(osta_name("Caf\u{e9}"), [8, b'C', b'a', b'f', 0xe9]);
        assert_eq!(osta_name("\u{1f3b5}"), [16, 0xd8, 0x3c, 0xdf, 0xb5]);
        let mut field = [0; 32];
        dstring(&mut field, 0, 32, &"\u{97f3}".repeat(20));
        assert_eq!(field[31], 31);
        assert_eq!(field[0], 16);
    }
}
