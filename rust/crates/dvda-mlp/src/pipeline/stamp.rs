use crate::metadata::Metadata;
use std::borrow::Cow;

pub(crate) struct MlpStampState<'a> {
    records: Cow<'a, [Metadata]>,
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
        records: Cow::Borrowed(records),
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

pub(crate) fn mlp_stamp_refresh(s: &mut MlpStampState<'_>, record: Metadata) -> bool {
    if s.failed || !s.start || s.records.len() != 1 || record.start != 0 || record.valid_bits != 0 {
        s.failed = true;
        return false;
    }
    let current = &s.records[0].packet;
    if record.packet.len() != current.len()
        || record.packet.get(..4) != current.get(..4)
        || record.packet.get(2..4) != Some(&[0x40, 0])
    {
        s.failed = true;
        return false;
    }
    if record.packet != *current {
        s.records.to_mut()[0] = record;
        s.byte = 0;
        s.bit = 7;
        s.prefix = 9;
        s.ones = 0;
    }
    true
}

impl MlpStampState<'_> {
    pub(crate) fn start_boundary(&self) -> bool {
        self.start
    }
}

pub(crate) fn mlp_stamp_next(s: &mut MlpStampState<'_>) -> Option<u32> {
    if s.failed || s.position >= s.limit {
        return None;
    }
    if s.index + 1 < s.records.len() && s.records[s.index + 1].start <= s.position {
        if !s.start || s.records[s.index + 1].start != s.position {
            s.failed = true;
            return None;
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
        return None;
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
    Some(bit)
}

pub(crate) fn mlp_stamp_complete(s: &MlpStampState<'_>) -> bool {
    !s.failed && s.position == s.limit && s.index + 1 == s.records.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustion_and_partial_packet_failure_do_not_advance() {
        let record = Metadata {
            start: 0,
            packet: vec![0, 0, 0x40, 0],
            valid_bits: 32,
        };
        let mut state = mlp_stamp_init(std::slice::from_ref(&record), 42).unwrap();
        for _ in 0..41 {
            assert!(mlp_stamp_next(&mut state).is_some());
        }
        assert_eq!(mlp_stamp_next(&mut state), None);
        assert_eq!(state.position, 41);
        assert!(state.failed);
        assert!(!mlp_stamp_complete(&state));
        assert_eq!(mlp_stamp_next(&mut state), None);
        assert_eq!(state.position, 41);

        let mut state = mlp_stamp_init(std::slice::from_ref(&record), 41).unwrap();
        for _ in 0..41 {
            assert!(mlp_stamp_next(&mut state).is_some());
        }
        assert!(mlp_stamp_complete(&state));
        assert_eq!(mlp_stamp_next(&mut state), None);
        assert!(!state.failed);
        assert!(mlp_stamp_complete(&state));
    }

    #[test]
    fn refresh_replaces_only_at_a_valid_stamp_boundary() {
        let original = Metadata {
            start: 0,
            packet: vec![0, 1, 0x40, 0, 0],
            valid_bits: 0,
        };
        let updated = Metadata {
            start: 0,
            packet: vec![0, 1, 0x40, 0, 1],
            valid_bits: 0,
        };
        let mut state = mlp_stamp_init(std::slice::from_ref(&original), 204).unwrap();
        assert_eq!(mlp_stamp_next(&mut state), Some(1));
        assert!(!mlp_stamp_refresh(&mut state, updated.clone()));
        let mut state = mlp_stamp_init(std::slice::from_ref(&original), 204).unwrap();
        let mut before = Vec::new();
        for _ in 0..50 {
            before.push(mlp_stamp_next(&mut state).unwrap());
        }
        assert!(state.start_boundary());
        assert!(mlp_stamp_refresh(&mut state, updated.clone()));
        let mut after = Vec::new();
        for _ in 0..50 {
            after.push(mlp_stamp_next(&mut state).unwrap());
        }
        let expected = crate::metadata::timeline(
            &[
                original,
                Metadata {
                    start: 50,
                    packet: updated.packet,
                    valid_bits: 0,
                },
            ],
            100,
        )
        .unwrap();
        assert_eq!(before, expected[..50]);
        assert_eq!(after, expected[50..]);
    }
}
