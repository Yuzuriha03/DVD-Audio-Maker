use crate::metadata::Metadata;

pub(crate) struct MlpStampState<'a> {
    records: &'a [Metadata],
    index: usize,
    position: u64,
    limit: u64,
    byte: usize,
    bit: u32,
    prefix: u32,
    ones: u32,
    start: bool,
    failed: bool,
}

pub(crate) fn mlp_stamp_init(records: &[Metadata], aus: u64) -> Option<MlpStampState<'_>> {
    if records.is_empty() || aus == 0 || records[0].start != 0 {
        return None;
    }
    for (i, r) in records.iter().enumerate() {
        let size = r.packet.len();
        if !(4..=65539).contains(&size)
            || r.start >= aus
            || (i != 0 && r.start <= records[i - 1].start)
            || r.packet[2] != 0x40
            || r.packet[3] != 0
        {
            return None;
        }
        let declared = usize::from(u16::from_be_bytes([r.packet[0], r.packet[1]])) + 4;
        if r.valid_bits != 0 {
            if i + 1 != records.len()
                || declared < size
                || r.valid_bits < 32
                || r.valid_bits as usize <= 8 * (size - 1)
                || r.valid_bits as usize > 8 * size
            {
                return None;
            }
        } else if declared != size {
            return None;
        }
        let mut period = 10u64;
        let mut ones = 0;
        for &byte in &r.packet {
            for bit in (0..8).rev() {
                if ones == 8 {
                    period += 1;
                    ones = 0;
                }
                period += 1;
                ones = if byte >> bit & 1 != 0 { ones + 1 } else { 0 };
            }
        }
        if ones == 8 {
            period += 1;
        }
        if i + 1 < records.len() && !(records[i + 1].start - r.start).is_multiple_of(period) {
            return None;
        }
    }
    Some(MlpStampState {
        records,
        index: 0,
        position: 0,
        limit: aus,
        byte: 0,
        bit: 7,
        prefix: 9,
        ones: 0,
        start: true,
        failed: false,
    })
}

pub(crate) fn mlp_stamp_next(s: &mut MlpStampState<'_>, result: &mut u32) -> bool {
    if s.failed || s.position >= s.limit {
        return false;
    }
    if s.index + 1 < s.records.len() && s.records[s.index + 1].start <= s.position {
        if !s.start || s.records[s.index + 1].start != s.position {
            s.failed = true;
            return false;
        }
        s.index += 1;
    }
    s.start = false;
    let r = &s.records[s.index];
    let bit;
    if s.prefix != 0 {
        bit = 1;
        s.prefix -= 1;
    } else if s.ones == 8 {
        bit = 0;
        s.ones = 0;
    } else if r.valid_bits != 0 && s.byte as u32 * 8 + 7 - s.bit >= r.valid_bits {
        s.failed = true;
        return false;
    } else if s.byte == r.packet.len() {
        bit = 0;
        s.byte = 0;
        s.bit = 7;
        s.prefix = 9;
        s.ones = 0;
        s.start = true;
    } else {
        bit = u32::from(r.packet[s.byte] >> s.bit & 1);
        s.ones = if bit != 0 { s.ones + 1 } else { 0 };
        if s.bit == 0 {
            s.bit = 7;
            s.byte += 1;
        } else {
            s.bit -= 1;
        }
    }
    s.position += 1;
    *result = bit;
    true
}

pub(crate) fn mlp_stamp_complete(s: &MlpStampState<'_>) -> bool {
    !s.failed && s.position == s.limit && s.index + 1 == s.records.len()
}
