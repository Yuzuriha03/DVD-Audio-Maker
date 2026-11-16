//! Parser for the private dvdauthor-data sector written by the SPU multiplexer.
//!
//! The multiplexer records subtitle timing, the 16-colour palette, highlight
//! colour groups and the button geometry/neighbour names in a private stream.
//! The VOB builder consumes that record when it fills the PCI button table, so
//! it is parsed into owned data structures here instead of C structs.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Colour {
    pub(crate) y: u8,
    pub(crate) cr: u8,
    pub(crate) cb: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Group {
    /// Four 16-bit colour entries (highlight index/alpha, select index/alpha).
    pub(crate) entries: [u16; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Button {
    pub(crate) name: String,
    pub(crate) auto_action: bool,
    pub(crate) group: u8,
    pub(crate) x0: u16,
    pub(crate) y0: u16,
    pub(crate) x1: u16,
    pub(crate) y1: u16,
    pub(crate) up: Option<String>,
    pub(crate) down: Option<String>,
    pub(crate) left: Option<String>,
    pub(crate) right: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DvdData {
    pub(crate) start_pts: u32,
    pub(crate) end_pts: u32,
    pub(crate) palette: Vec<Colour>,
    pub(crate) groups: Vec<Group>,
    pub(crate) buttons: Vec<Button>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> Result<u8, String> {
        let value = *self.bytes.get(self.at).ok_or("truncated dvdauthor-data")?;
        self.at += 1;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes = self
            .bytes
            .get(self.at..self.at + 2)
            .ok_or("truncated dvdauthor-data")?;
        self.at += 2;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or("truncated dvdauthor-data")?;
        self.at += 4;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.at;
        while self.u8()? != 0 {}
        Ok(String::from_utf8_lossy(&self.bytes[start..self.at - 1]).into_owned())
    }
}

/// Parses one dvdauthor-data payload (the bytes after the private-stream header).
pub(crate) fn parse(payload: &[u8]) -> Result<DvdData, String> {
    let mut reader = Reader {
        bytes: payload,
        at: 0,
    };
    let tag = reader.string()?;
    if tag != "dvdauthor-data" {
        return Err(format!("unexpected dvdauthor-data tag {tag:?}"));
    }
    if reader.u8()? != 2 || reader.u8()? != 1 {
        return Err("unsupported dvdauthor-data version".into());
    }
    let stream = reader.u8()?;
    let start_pts = reader.u32()?;
    let end_pts = reader.u32()?;
    let mut data = DvdData {
        start_pts,
        end_pts,
        ..DvdData::default()
    };
    let _palette_marker = reader.u8()?;
    let numpal = reader.u8()?;
    for _ in 0..numpal {
        let y = reader.u8()?;
        let cr = reader.u8()?;
        let cb = reader.u8()?;
        data.palette.push(Colour { y, cr, cb });
    }
    if reader.at < payload.len() && reader.bytes[reader.at] == 2 {
        reader.u8()?;
        let numgroups = reader.u8()?;
        for _ in 0..numgroups {
            data.groups.push(Group {
                entries: [reader.u16()?, reader.u16()?, reader.u16()?, reader.u16()?],
            });
        }
    }
    if reader.at < payload.len() && reader.bytes[reader.at] == 3 {
        reader.u8()?;
        let count = reader.u8()?;
        for _ in 0..count {
            let name = reader.string()?;
            let _unknown = reader.u16()?;
            let auto_action = reader.u8()? != 0;
            let group = reader.u8()?;
            let x0 = reader.u16()?;
            let y0 = reader.u16()?;
            let x1 = reader.u16()?;
            let y1 = reader.u16()?;
            let neighbour = |reader: &mut Reader| -> Result<Option<String>, String> {
                let value = reader.string()?;
                Ok(if value.is_empty() { None } else { Some(value) })
            };
            let up = neighbour(&mut reader)?;
            let down = neighbour(&mut reader)?;
            let left = neighbour(&mut reader)?;
            let right = neighbour(&mut reader)?;
            data.buttons.push(Button {
                name,
                auto_action,
                group,
                x0,
                y0,
                x1,
                y1,
                up,
                down,
                left,
                right,
            });
        }
    }
    let _ = stream;
    Ok(data)
}

/// Parses the payload of a whole PAD sector if it carries dvdauthor data.
pub(crate) fn parse_sector(sector: &[u8]) -> Option<Result<DvdData, String>> {
    if sector.len() < 40 || sector[14..18] != [0, 0, 1, 0xbe] {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([sector[18], sector[19]]));
    let payload = sector.get(20..20 + length)?;
    Some(parse(payload))
}

/// Location of the legacy C `dvdauthor` acceptance output that the
/// navigation tests use as a bit-exact baseline.
///
/// The baseline (`menu.mpg` plus `导航 日本語/AUDIO_TS/AUDIO_TS.VOB`) is a build
/// artifact of the retired C implementation, so it is not tracked by Git. Point
/// `DVDA_MENU_BASELINE_ROOT` at a directory that holds those files; otherwise
/// the repository-relative `build/menu-accept-old2` is used.
#[cfg(test)]
pub(crate) fn baseline_root() -> std::path::PathBuf {
    std::env::var_os("DVDA_MENU_BASELINE_ROOT").map_or_else(
        || {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("build")
                .join("menu-accept-old2")
        },
        std::path::PathBuf::from,
    )
}

/// Resolves one baseline file and names the expected path when it is absent.
/// A baseline-backed test must fail instead of passing without its baseline.
#[cfg(test)]
#[track_caller]
pub(crate) fn baseline_path(relative: &str) -> std::path::PathBuf {
    let path = baseline_root().join(relative);
    assert!(
        path.is_file(),
        "required C dvdauthor baseline fixture is missing: {}\n\
         regenerate the menu-accept-old2 acceptance output or set \
         DVDA_MENU_BASELINE_ROOT to the directory that contains it",
        path.display()
    );
    path
}

#[cfg(test)]
#[track_caller]
pub(crate) fn baseline_file(relative: &str) -> Vec<u8> {
    let path = baseline_path(relative);
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!("baseline fixture {} is unreadable: {error}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires build/menu-accept-old2 baseline; set DVDA_MENU_BASELINE_ROOT"]
    fn parses_the_spu_private_sector() {
        let bytes = baseline_file("menu.mpg");
        let sector = bytes
            .get(119 * 2048..120 * 2048)
            .expect("baseline menu.mpg must contain sector 119")
            .to_vec();
        let data = parse_sector(&sector)
            .expect("dvdauthor-data sector")
            .unwrap();
        assert_eq!(data.start_pts, 48_600);
        assert_eq!(data.palette.len(), 3);
        assert_eq!(data.buttons.len(), 1);
        let button = &data.buttons[0];
        assert_eq!(button.name, "button01");
        assert_eq!(
            (button.x0, button.y0, button.x1, button.y1),
            (20, 56, 700, 100)
        );
        assert!(!button.auto_action);
        assert_eq!(button.group, 1);
        assert_eq!(button.up.as_deref(), Some("button01"));
    }
}
