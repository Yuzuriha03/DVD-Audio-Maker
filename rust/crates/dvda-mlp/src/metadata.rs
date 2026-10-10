use crate::{Error, stamp};
#[derive(Clone, Debug)]
pub struct Metadata {
    pub start: u64,
    pub packet: Vec<u8>,
    pub valid_bits: u32,
}
pub fn timeline(records: &[Metadata], count: usize) -> Result<Vec<u32>, Error> {
    if records.is_empty() || records[0].start != 0 {
        return Err(Error::InvalidMetadata);
    }
    let mut output = Vec::with_capacity(count);
    for (index, r) in records.iter().enumerate() {
        let p = &r.packet;
        if p.len() < 4
            || p.len() > 65539
            || p[2..4] != [0x40, 0]
            || r.start >= count as u64
            || (index > 0 && r.start <= records[index - 1].start)
        {
            return Err(Error::InvalidMetadata);
        }
        let declared = usize::from(u16::from_be_bytes([p[0], p[1]])) + 4;
        if (r.valid_bits == 0 && declared != p.len())
            || (r.valid_bits != 0
                && (index + 1 != records.len()
                    || declared < p.len()
                    || r.valid_bits < 32
                    || r.valid_bits as usize <= 8 * (p.len() - 1)
                    || r.valid_bits as usize > 8 * p.len()))
        {
            return Err(Error::InvalidMetadata);
        }
        let full = stamp(p);
        let end = records.get(index + 1).map_or(count as u64, |n| n.start);
        if end <= r.start
            || end > count as u64
            || (index + 1 < records.len() && !(end - r.start).is_multiple_of(full.len() as u64))
        {
            return Err(Error::InvalidMetadata);
        }
        if r.valid_bits == 0 {
            output.extend(full.iter().copied().cycle().take((end - r.start) as usize));
        } else {
            let mut prefix = vec![1; 9];
            let mut ones = 0;
            for bit in 0..r.valid_bits as usize {
                if ones == 8 {
                    prefix.push(0);
                    ones = 0;
                }
                let value = u32::from((p[bit / 8] >> (7 - bit % 8)) & 1);
                prefix.push(value);
                ones = if value == 1 { ones + 1 } else { 0 };
            }
            if ones == 8 {
                prefix.push(0);
            }
            if (end - r.start) as usize > prefix.len() {
                return Err(Error::InvalidMetadata);
            }
            output.extend_from_slice(&prefix[..(end - r.start) as usize]);
        }
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_and_partial_records() {
        let packet = vec![0, 0, 0x40, 0];
        let records = [
            Metadata {
                start: 0,
                packet: packet.clone(),
                valid_bits: 0,
            },
            Metadata {
                start: 42,
                packet,
                valid_bits: 32,
            },
        ];
        assert_eq!(timeline(&records, 83).unwrap().len(), 83);
        assert_eq!(timeline(&records, 84), Err(Error::InvalidMetadata));
        let mut invalid = records;
        invalid[1].start = 41;
        assert_eq!(timeline(&invalid, 82), Err(Error::InvalidMetadata));
    }
}
