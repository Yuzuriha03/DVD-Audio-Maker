use crate::{Error, encoder::stamp, metadata::Metadata};

#[cfg(any(test, not(target_arch = "x86_64")))]
struct StampPacket {
    start: u64,
    bits: Vec<u32>,
    partial: bool,
}

/// Validates metadata packets and, on the fallback path, advances their bit cursor.
pub(crate) struct MetadataCursor {
    #[cfg(any(test, not(target_arch = "x86_64")))]
    packets: Vec<StampPacket>,
    #[cfg(any(test, not(target_arch = "x86_64")))]
    record: usize,
    #[cfg(any(test, not(target_arch = "x86_64")))]
    position: usize,
}
impl MetadataCursor {
    pub(crate) fn new(records: &[Metadata], count: usize) -> Result<Self, Error> {
        if records.is_empty() || records[0].start != 0 {
            return Err(Error::InvalidMetadata);
        }
        #[cfg(any(test, not(target_arch = "x86_64")))]
        let mut packets = Vec::new();
        for (i, r) in records.iter().enumerate() {
            let p = &r.packet;
            if !(4..=65539).contains(&p.len()) || p[2..4] != [0x40, 0] || r.start >= count as u64 {
                return Err(Error::InvalidMetadata);
            }
            let declared = usize::from(u16::from_be_bytes([p[0], p[1]])) + 4;
            let partial = r.valid_bits != 0;
            if (!partial && declared != p.len())
                || (partial
                    && (i + 1 != records.len()
                        || declared < p.len()
                        || r.valid_bits < 32
                        || r.valid_bits as usize <= 8 * (p.len() - 1)
                        || r.valid_bits as usize > 8 * p.len()))
            {
                return Err(Error::InvalidMetadata);
            }
            let full = stamp(p);
            let end = records.get(i + 1).map_or(count as u64, |n| n.start);
            if end <= r.start
                || end > count as u64
                || (i + 1 < records.len() && !(end - r.start).is_multiple_of(full.len() as u64))
            {
                return Err(Error::InvalidMetadata);
            }
            let bits = if partial {
                let mut bits = vec![1; 9];
                let mut ones = 0;
                for n in 0..r.valid_bits as usize {
                    if ones == 8 {
                        bits.push(0);
                        ones = 0;
                    }
                    let bit = u32::from((p[n / 8] >> (7 - n % 8)) & 1);
                    bits.push(bit);
                    ones = if bit == 1 { ones + 1 } else { 0 };
                }
                if ones == 8 {
                    bits.push(0);
                }
                if end - r.start > bits.len() as u64 {
                    return Err(Error::InvalidMetadata);
                }
                bits
            } else {
                full
            };
            #[cfg(any(test, not(target_arch = "x86_64")))]
            packets.push(StampPacket {
                start: r.start,
                bits,
                partial,
            });
            #[cfg(all(not(test), target_arch = "x86_64"))]
            let _ = bits;
        }
        Ok(Self {
            #[cfg(any(test, not(target_arch = "x86_64")))]
            packets,
            #[cfg(any(test, not(target_arch = "x86_64")))]
            record: 0,
            #[cfg(any(test, not(target_arch = "x86_64")))]
            position: 0,
        })
    }
    #[cfg(any(test, not(target_arch = "x86_64")))]
    pub(crate) fn next(&mut self, au: usize) -> u32 {
        if self
            .packets
            .get(self.record + 1)
            .is_some_and(|p| p.start == au as u64)
        {
            self.record += 1;
            self.position = 0;
        }
        let packet = &self.packets[self.record];
        let bit = packet.bits[self.position];
        self.position += 1;
        if self.position == packet.bits.len() && !packet.partial {
            self.position = 0;
        }
        bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_cursor_matches_timeline_across_repeat_and_partial_tail() {
        let packet = vec![0, 1, 0x40, 0, 0xff];
        let boundary = stamp(&packet).len() * 2;
        let records = [
            Metadata {
                start: 0,
                packet: packet.clone(),
                valid_bits: 0,
            },
            Metadata {
                start: boundary as u64,
                packet,
                valid_bits: 37,
            },
        ];
        let count = boundary + 40;
        let expected = crate::metadata::timeline(&records, count).unwrap();
        let mut stamps = MetadataCursor::new(&records, count).unwrap();
        let actual: Vec<_> = (0..count).map(|au| stamps.next(au)).collect();
        assert_eq!(actual, expected);
    }
}
