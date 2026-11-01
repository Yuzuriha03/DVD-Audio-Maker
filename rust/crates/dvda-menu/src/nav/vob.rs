//! Navigation pack (PCI/DSI) parsing for the VOB migration.
//!
//! Slice 2 foundation: reads the 2048-byte sectors of a navigation VOB and
//! exposes the PCI button table and DSI addressing fields the builder must
//! produce. The writer side will be added together with the VOBU grouping;
//! having the reader first lets the port be checked against the C-produced
//! baseline VOB sector by sector.

#![allow(dead_code)]

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ButtonInfo {
    pub(crate) group: u8,
    /// Button rectangle in DVD 12-bit coordinates.
    pub(crate) x_start: u16,
    pub(crate) x_end: u16,
    pub(crate) y_start: u16,
    pub(crate) y_end: u16,
    pub(crate) auto_action: bool,
    /// Neighbour button numbers, 1-based, as written to the VOB.
    pub(crate) up: u8,
    pub(crate) down: u8,
    pub(crate) left: u8,
    pub(crate) right: u8,
    pub(crate) command: [u8; 8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pci {
    pub(crate) sector: u32,
    pub(crate) start_pts: u32,
    pub(crate) end_pts: u32,
    pub(crate) sequence_end_pts: u32,
    pub(crate) cell_elapsed: u32,
    pub(crate) highlight_status: u16,
    pub(crate) button_count: u8,
    pub(crate) buttons: Vec<ButtonInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dsi {
    pub(crate) scr: u32,
    pub(crate) sector: u32,
    pub(crate) last_reference_offsets: [u32; 3],
    pub(crate) vob_id: u16,
    pub(crate) cell_id: u8,
    pub(crate) cell_elapsed: u32,
    pub(crate) vob_start_pts: u32,
    pub(crate) vob_end_pts: u32,
    pub(crate) next_vobu: u32,
    pub(crate) previous_vobu: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NavPack {
    pub(crate) sector: u32,
    pub(crate) pci: Pci,
    pub(crate) dsi: Dsi,
}

const SECTOR: usize = 2048;
const PACK_START: [u8; 4] = [0, 0, 1, 0xba];
const PRIVATE_2: [u8; 4] = [0, 0, 1, 0xbf];

fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn be16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn button_table(sector: &[u8], count: u8) -> Result<Vec<ButtonInfo>, String> {
    let mut buttons = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let at = 0xbb + index * 18;
        let entry = sector.get(at..at + 18).ok_or("PCI button table out of range")?;
        let x_start = u16::from(entry[0] & 0x3f) << 4 | u16::from(entry[1] >> 4);
        let x_end = u16::from(entry[1] & 0x0f) << 8 | u16::from(entry[2]);
        let y_start = u16::from(entry[3] & 0x3f) << 4 | u16::from(entry[4] >> 4);
        let y_end = u16::from(entry[4] & 0x0f) << 8 | u16::from(entry[5]);
        let mut command = [0u8; 8];
        command.copy_from_slice(&entry[10..18]);
        buttons.push(ButtonInfo {
            group: entry[0] >> 6,
            x_start,
            x_end,
            y_start,
            y_end,
            auto_action: entry[3] & 0x40 != 0,
            up: entry[6],
            down: entry[7],
            left: entry[8],
            right: entry[9],
            command,
        });
    }
    Ok(buttons)
}

fn parse_nav_pack(sector_index: usize, sector: &[u8]) -> Result<NavPack, String> {
    if sector.len() != SECTOR {
        return Err("short sector".into());
    }
    if sector[..4] != PACK_START {
        return Err(format!("sector {sector_index} does not start with a PACK header"));
    }
    if sector[0x26..0x2a] != PRIVATE_2 || sector[0x400..0x404] != PRIVATE_2 {
        return Err(format!("sector {sector_index} is not a navigation pack"));
    }
    if sector[0x2c] != 0 || sector[0x406] != 1 {
        return Err(format!("sector {sector_index} has unexpected NAV substream IDs"));
    }
    let button_count = sector[0x9e];
    let pci = Pci {
        sector: be32(&sector[0x2d..]),
        start_pts: be32(&sector[0x39..]),
        end_pts: be32(&sector[0x3d..]),
        sequence_end_pts: be32(&sector[0x41..]),
        cell_elapsed: be32(&sector[0x45..]),
        highlight_status: be16(&sector[0x8d..]),
        button_count,
        buttons: button_table(sector, button_count)?,
    };
    let dsi = Dsi {
        scr: be32(&sector[0x407..]),
        sector: be32(&sector[0x40b..]),
        last_reference_offsets: [
            be32(&sector[0x413..]),
            be32(&sector[0x417..]),
            be32(&sector[0x41b..]),
        ],
        vob_id: be16(&sector[0x41f..]),
        cell_id: sector[0x422],
        cell_elapsed: be32(&sector[0x423..]),
        vob_start_pts: be32(&sector[0x433..]),
        vob_end_pts: be32(&sector[0x437..]),
        next_vobu: be32(&sector[0x541..]),
        previous_vobu: be32(&sector[0x545..]),
    };
    Ok(NavPack { sector: sector_index as u32, pci, dsi })
}

/// Extracts every navigation pack from a VOB byte image.
pub(crate) fn parse_nav_packs(vob: &[u8]) -> Result<Vec<NavPack>, String> {
    if !vob.len().is_multiple_of(SECTOR) {
        return Err("VOB size is not a multiple of 2048 bytes".into());
    }
    let (sectors, _) = vob.as_chunks::<SECTOR>();
    sectors
        .iter()
        .enumerate()
        .filter(|(_, sector)| sector[..4] == PACK_START && sector.get(0x26..0x2a) == Some(&PRIVATE_2))
        .map(|(index, sector)| parse_nav_pack(index, sector))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Optional preserved C baseline. Set DVDA_MENU_NAV_BASELINE to an
    /// AUDIO_TS.VOB produced by the historical C implementation to run this
    /// differential parser check; it is skipped when no baseline is present.
    fn baseline() -> Option<Vec<u8>> {
        let path = std::env::var_os("DVDA_MENU_NAV_BASELINE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    r"C:\Users\yyz57\Documents\Visual Studio Code\DVD-Audio-Maker\build\menu-accept-old2\导航 日本語\AUDIO_TS\AUDIO_TS.VOB",
                )
            });
        std::fs::read(path).ok()
    }

    #[test]
    fn baseline_nav_packs_are_structured_as_expected() {
        let Some(vob) = baseline() else {
            eprintln!(
                "skipping baseline NAV pack check: set DVDA_MENU_NAV_BASELINE to a preserved C AUDIO_TS.VOB"
            );
            return;
        };
        let packs = parse_nav_packs(&vob).unwrap();
        assert_eq!(packs.len(), 4, "the C baseline has four VOBUs with NAV packs");
        let sectors: Vec<u32> = packs.iter().map(|pack| pack.sector).collect();
        assert_eq!(sectors, [0, 235, 480, 725]);
        for (index, pack) in packs.iter().enumerate() {
            assert_eq!(pack.pci.sector, sectors[index]);
            assert_eq!((pack.dsi.vob_id, pack.dsi.cell_id), (1, 1));
        }
        let points: Vec<(u32, u32)> = packs.iter().map(|pack| (pack.pci.start_pts, pack.pci.end_pts)).collect();
        assert_eq!(
            points,
            [(48600, 102600), (102600, 156600), (156600, 210600), (210600, 229560)]
        );
        for pack in &packs {
            assert_eq!(pack.pci.button_count, 1);
            assert_eq!(pack.pci.buttons.len(), 1);
            let button = &pack.pci.buttons[0];
            assert_eq!((button.x_start, button.x_end), (20, 700));
            assert_eq!((button.y_start, button.y_end), (56, 100));
            assert_eq!((button.up, button.down, button.left, button.right), (1, 1, 1, 1));
            assert_eq!(
                button.command,
                [0x20, 0x04, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x01]
            );
        }
    }
}
