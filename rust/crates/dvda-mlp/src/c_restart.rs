use crate::Bits;

#[derive(Clone, Debug, Default)]
#[repr(C)]
pub(crate) struct MlpRestart {
    pub timing: u32,
    pub minimum_channel: u32,
    pub maximum_channel: u32,
    pub dither_shift: u32,
    pub seed: u32,
    pub maximum_shift: u32,
    pub maximum_lsbs: u32,
    pub maximum_bits: u32,
    pub lossless_check: u32,
    pub assignment: [u32; 16],
}

fn checksum(writer: &Bits) -> u32 {
    let mut crc = 0x31eau32;
    for &word in writer.words.iter().skip(1) {
        let t = crc >> 9;
        crc = ((((crc & 0x1ff) << 15) ^ t) << 1) ^ word ^ t;
    }
    for _ in 0..17 {
        if crc & 0x01000000 != 0 {
            crc ^= 0x011d0000;
        }
        crc <<= 1;
    }
    if writer.width != 0 {
        crc ^= writer.pending << (17 - writer.width);
    }
    for _ in 0..writer.width {
        if crc & 0x01000000 != 0 {
            crc ^= 0x011d0000;
        }
        crc <<= 1;
    }
    crc >> 17
}

pub(crate) fn mlp_restart_put(writer: &mut Bits, h: &MlpRestart, primary: bool) -> Result<(), ()> {
    if !writer.words.is_empty()
        || writer.width != 0
        || writer.pending != 0
        || h.timing > 65535
        || h.minimum_channel > h.maximum_channel
        || h.maximum_channel > 15
        || h.dither_shift > 15
        || h.seed > 0x7fffff
        || h.maximum_lsbs > 31
        || h.maximum_bits > 31
        || h.lossless_check > 255
    {
        return Err(());
    }
    if !primary
        && h.assignment[..=h.maximum_channel as usize]
            .iter()
            .any(|&v| v > 63)
    {
        return Err(());
    }
    let mut local = Bits::default();
    local.put(3, 2);
    local.put(0x31ea, 14);
    local.put(h.timing, 16);
    local.put(h.minimum_channel, 4);
    local.put(h.maximum_channel, 4);
    local.put(h.maximum_channel, 4);
    local.put(h.dither_shift, 4);
    local.put(h.seed >> 16, 7);
    local.put(h.seed & 65535, 16);
    local.put(h.maximum_shift & 15, 4);
    local.put(h.maximum_lsbs, 5);
    local.put(33 * h.maximum_bits, 10);
    local.put(0, 1);
    local.put(h.lossless_check, 8);
    local.put(0, 16);
    for i in 0..=h.maximum_channel {
        local.put(if primary { i } else { h.assignment[i as usize] }, 6);
    }
    local.put(checksum(&local), 8);
    *writer = local;
    Ok(())
}
