//! Bounded PE32+ reader used before packaging untrusted or stale build outputs.
use std::collections::BTreeSet;

pub fn imports(bytes: &[u8], dll: bool) -> Result<BTreeSet<String>, String> {
    let pe = Pe::new(bytes, dll)?;
    let mut result = BTreeSet::new();
    for (index, width, name_offset) in [(1, 20, 12), (13, 32, 4)] {
        let (rva, size) = pe.directory(index)?;
        if rva == 0 && size == 0 {
            continue;
        }
        if rva == 0 || size < width {
            return Err("Invalid PE import directory".into());
        }
        let directory = pe.rva(rva, size)?;
        let mut terminated = false;
        for row in directory.chunks_exact(width as usize) {
            if row.iter().all(|value| *value == 0) {
                terminated = true;
                break;
            }
            let mut name_rva = u32_at(row, name_offset)?;
            if index == 13 {
                match u32_at(row, 0)? {
                    1 => {}
                    0 => {
                        name_rva = u32::try_from(
                            u64::from(name_rva)
                                .checked_sub(pe.base)
                                .ok_or("Invalid delay-import address")?,
                        )
                        .map_err(|_| "Invalid delay-import address")?;
                    }
                    _ => return Err("Invalid delay-import attributes".into()),
                }
            }
            let name = pe.c_string(name_rva)?;
            if !valid_name(&name, "dll") {
                return Err(format!("Invalid PE import name: {name}"));
            }
            result.insert(name.to_ascii_lowercase());
        }
        if !terminated {
            return Err("Unterminated PE import directory".into());
        }
    }
    Ok(result)
}

pub fn valid_name(name: &str, extension: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    !name.is_empty()
        && !reserved
        && name.len() < 256
        && name
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || b"._-".contains(&value))
        && !name.starts_with('.')
        && name
            .to_ascii_lowercase()
            .ends_with(&format!(".{extension}"))
}

struct Section {
    address: u32,
    length: u32,
    offset: u32,
}
struct Pe<'a> {
    bytes: &'a [u8],
    directories: &'a [u8],
    sections: Vec<Section>,
    headers: u32,
    base: u64,
}

impl<'a> Pe<'a> {
    fn new(bytes: &'a [u8], dll: bool) -> Result<Self, String> {
        if bytes.get(..2) != Some(b"MZ") {
            return Err("Missing DOS header".into());
        }
        let offset = u32_at(bytes, 0x3c)? as usize;
        let header = slice(bytes, offset, 24)?;
        if &header[..4] != b"PE\0\0" || u16_at(header, 4)? != 0x8664 {
            return Err("Native component must be Windows x64".into());
        }
        let flags = u16_at(header, 22)?;
        if flags & 2 == 0 || (flags & 0x2000 != 0) != dll {
            return Err("PE executable/DLL type does not match its file name".into());
        }
        let count = u16_at(header, 6)? as usize;
        if count == 0 || count > 96 {
            return Err("Invalid PE section count".into());
        }
        let optional_size = u16_at(header, 20)? as usize;
        let optional = slice(bytes, offset + 24, optional_size)?;
        if u16_at(optional, 0)? != 0x20b || optional_size < 112 {
            return Err("Native component must use PE32+".into());
        }
        let directory_count = u32_at(optional, 108)? as usize;
        let directories = slice(
            optional,
            112,
            directory_count.checked_mul(8).ok_or("PE overflow")?,
        )?;
        let headers = u32_at(optional, 60)?;
        slice(bytes, 0, headers as usize)?;
        let mut sections = Vec::new();
        for row in slice(bytes, offset + 24 + optional_size, count * 40)?
            .as_chunks::<40>()
            .0
        {
            let length = u32_at(row, 16)?;
            let address = u32_at(row, 12)?;
            let offset = u32_at(row, 20)?;
            slice(bytes, offset as usize, length as usize)?;
            address
                .checked_add(length)
                .ok_or("PE section address overflow")?;
            if length > 0
                && (address < headers
                    || sections.iter().any(|s: &Section| {
                        s.address < address + length && address < s.address + s.length
                    }))
            {
                return Err("Ambiguous PE section addresses".into());
            }
            sections.push(Section {
                address,
                length,
                offset,
            });
        }
        let pe = Self {
            bytes,
            directories,
            sections,
            headers,
            base: u64_at(optional, 24)?,
        };
        if pe.directory(14)? != (0, 0) {
            return Err("Managed PE components are not supported".into());
        }
        Ok(pe)
    }

    fn directory(&self, index: usize) -> Result<(u32, u32), String> {
        if index >= self.directories.len() / 8 {
            return Ok((0, 0));
        }
        Ok((
            u32_at(self.directories, index * 8)?,
            u32_at(self.directories, index * 8 + 4)?,
        ))
    }
    fn rva(&self, address: u32, length: u32) -> Result<&'a [u8], String> {
        let end = address.checked_add(length).ok_or("PE address overflow")?;
        if address < self.headers && end <= self.headers {
            return slice(self.bytes, address as usize, length as usize);
        }
        for section in &self.sections {
            if address >= section.address && end <= section.address + section.length {
                let offset = section.offset as usize + (address - section.address) as usize;
                return slice(self.bytes, offset, length as usize);
            }
        }
        Err("PE RVA is outside file-backed sections".into())
    }
    fn c_string(&self, address: u32) -> Result<String, String> {
        let mut result = Vec::new();
        for index in 0..256 {
            let value = self.rva(address.checked_add(index).ok_or("PE name overflow")?, 1)?[0];
            if value == 0 {
                return String::from_utf8(result).map_err(|_| "Invalid PE import string".into());
            }
            result.push(value);
        }
        Err("Unterminated PE import string".into())
    }
}

fn slice(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
    bytes
        .get(offset..offset.checked_add(length).ok_or("PE range overflow")?)
        .ok_or_else(|| "Truncated PE structure".into())
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        slice(bytes, offset, 2)?.try_into().unwrap(),
    ))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        slice(bytes, offset, 4)?.try_into().unwrap(),
    ))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        slice(bytes, offset, 8)?.try_into().unwrap(),
    ))
}

#[cfg(test)]
pub(crate) fn fixture(dll: bool, ordinary: &[&str], delay: &[&str]) -> Vec<u8> {
    let mut b = vec![0u8; 8192];
    b[..2].copy_from_slice(b"MZ");
    b[0x3c..0x40].copy_from_slice(&128u32.to_le_bytes());
    b[128..132].copy_from_slice(b"PE\0\0");
    b[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
    b[134..136].copy_from_slice(&1u16.to_le_bytes());
    b[148..150].copy_from_slice(&240u16.to_le_bytes());
    b[150..152].copy_from_slice(&(2u16 | if dll { 0x2000 } else { 0 }).to_le_bytes());
    b[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
    b[176..184].copy_from_slice(&0x140000000u64.to_le_bytes());
    b[212..216].copy_from_slice(&512u32.to_le_bytes());
    b[260..264].copy_from_slice(&16u32.to_le_bytes());
    b[404..408].copy_from_slice(&512u32.to_le_bytes());
    b[408..412].copy_from_slice(&7680u32.to_le_bytes());
    b[412..416].copy_from_slice(&512u32.to_le_bytes());
    let mut name_at = 4096;
    for (names, index, width, at, name_offset) in
        [(ordinary, 1, 20, 512, 12), (delay, 13, 32, 2048, 4)]
    {
        if names.is_empty() {
            continue;
        }
        b[264 + index * 8..268 + index * 8].copy_from_slice(&(at as u32).to_le_bytes());
        b[268 + index * 8..272 + index * 8]
            .copy_from_slice(&(((names.len() + 1) * width) as u32).to_le_bytes());
        for (i, name) in names.iter().enumerate() {
            let row = at + i * width;
            if index == 13 {
                b[row..row + 4].copy_from_slice(&1u32.to_le_bytes());
            }
            b[row + name_offset..row + name_offset + 4]
                .copy_from_slice(&(name_at as u32).to_le_bytes());
            b[name_at..name_at + name.len()].copy_from_slice(name.as_bytes());
            name_at += name.len() + 1;
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_and_delay_imports_are_bounded_and_normalized() {
        let bytes = fixture(true, &["KERNEL32.dll"], &["delayed.dll"]);
        assert_eq!(
            imports(&bytes, true).unwrap(),
            BTreeSet::from(["kernel32.dll".into(), "delayed.dll".into()])
        );
        let mut malformed = bytes.clone();
        malformed[512 + 12..512 + 16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(imports(&malformed, true).is_err());
        let mut unterminated = bytes.clone();
        unterminated[264 + 8 + 4..264 + 8 + 8].copy_from_slice(&20u32.to_le_bytes());
        assert!(imports(&unterminated, true).is_err());
        let mut bad_delay = bytes.clone();
        bad_delay[2048..2052].copy_from_slice(&2u32.to_le_bytes());
        assert!(imports(&bad_delay, true).is_err());
        let mut bad_name = bytes.clone();
        bad_name[4096] = b'/';
        assert!(imports(&bad_name, true).is_err());
        let mut no_nul = bytes.clone();
        no_nul[4096..4352].fill(b'a');
        assert!(imports(&no_nul, true).is_err());
        for length in [0, 64, 131, 260, 410, 4098] {
            assert!(imports(&bytes[..length], true).is_err());
        }
    }
    #[test]
    fn native_x64_and_file_kind_are_mandatory() {
        let bytes = fixture(true, &[], &[]);
        assert!(imports(&bytes, false).is_err());
        for (offset, value) in [(132, 0x14cu32), (264 + 14 * 8, 1u32)] {
            let mut b = bytes.clone();
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(imports(&b, true).is_err());
        }
    }
}
