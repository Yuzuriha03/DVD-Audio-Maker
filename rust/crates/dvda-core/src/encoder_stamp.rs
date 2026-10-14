//! Project build identity, pseudonymous machine fingerprint and live local time.
use crate::media::Failure;
use dvda_native::encoder::Stamp;

fn field(packet: &mut Vec<u8>, kind: u16, value: &[u8]) {
    packet.extend_from_slice(
        &u16::try_from(value.len())
            .expect("bounded stamp field")
            .to_be_bytes(),
    );
    packet.extend_from_slice(&kind.to_be_bytes());
    packet.extend_from_slice(value);
}

fn packet(time: &[u8; 20], machine: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    for (kind, value) in [
        (0x1000, env!("DVDA_ENCODER_ID").as_bytes()),
        (0x1001, env!("DVDA_ENCODER_TARGET").as_bytes()),
        (0x1002, env!("DVDA_ENCODER_COMPILER").as_bytes()),
        (0x1100, machine),
        (0x1400, time.as_slice()),
    ] {
        field(&mut payload, kind, value);
    }
    let mut output = Vec::new();
    field(&mut output, 0x4000, &payload);
    output
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetComputerNameW(buffer: *mut u16, size: *mut u32) -> i32;
}
fn machine_fingerprint(name: &[u16]) -> String {
    let mut identity = b"DVD-Audio-Maker:computer-name-utf16le:v1\0".to_vec();
    for unit in name {
        identity.extend_from_slice(&unit.to_le_bytes());
    }
    crate::hash::hex_digest(&identity)
}
fn machine_identity() -> Result<String, Failure> {
    let mut name = [0u16; 256];
    let mut length = name.len() as u32;
    // SAFETY: length advertises the writable buffer capacity in UTF-16 units.
    if unsafe { GetComputerNameW(name.as_mut_ptr(), &mut length) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let name = name
        .get(..length as usize)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Failure::new("InvalidData", "Invalid computer name length"))?;
    Ok(machine_fingerprint(name))
}

pub(crate) fn current() -> Result<Stamp, Failure> {
    let machine = machine_identity()?;
    Ok(Stamp {
        start: 0,
        packet: packet(&timestamp()?, machine.as_bytes()),
        valid_bits: 0,
    })
}

#[repr(C)]
#[derive(Default)]
struct LocalTime {
    year: u16,
    month: u16,
    weekday: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}
fn format_time(now: &LocalTime) -> Result<[u8; 20], Failure> {
    let invalid = || Failure::new("InvalidData", "Invalid local encoding time");
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = now
        .month
        .checked_sub(1)
        .and_then(|i| months.get(i as usize))
        .ok_or_else(invalid)?;
    if !(1..=9999).contains(&now.year)
        || !(1..=31).contains(&now.day)
        || now.hour > 23
        || now.minute > 59
        || now.second > 59
    {
        return Err(invalid());
    }
    let text = format!(
        "{month} {:02} {:04} {:02}:{:02}:{:02}",
        now.day, now.year, now.hour, now.minute, now.second
    );
    text.as_bytes().try_into().map_err(|_| invalid())
}
fn timestamp() -> Result<[u8; 20], Failure> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut LocalTime);
    }
    let mut now = LocalTime::default();
    // SAFETY: LocalTime has the Win32 SYSTEMTIME layout and is writable.
    unsafe { GetLocalTime(&mut now) };
    format_time(&now)
}

#[cfg(feature = "rust-mlp")]
pub(crate) struct LiveStamp {
    packet: Vec<u8>,
    time_offset: usize,
}
#[cfg(feature = "rust-mlp")]
impl LiveStamp {
    pub(crate) fn new(stamp: &Stamp) -> Result<Self, Failure> {
        let invalid = || Failure::new("InvalidData", "Invalid live timestamp packet");
        let bytes = &stamp.packet;
        if stamp.start != 0
            || stamp.valid_bits != 0
            || bytes.len() < 4
            || bytes[2..4] != [0x40, 0]
            || usize::from(u16::from_be_bytes([bytes[0], bytes[1]])) + 4 != bytes.len()
        {
            return Err(invalid());
        }
        let mut at = 4;
        let mut time_offset = None;
        while at < bytes.len() {
            let header = bytes.get(at..at + 4).ok_or_else(invalid)?;
            let length = usize::from(u16::from_be_bytes([header[0], header[1]]));
            let end = at + 4 + length;
            bytes.get(at..end).ok_or_else(invalid)?;
            if header[2..4] == [0x14, 0] {
                if length != 20 || time_offset.is_some() {
                    return Err(invalid());
                }
                time_offset = Some(at + 4);
            }
            at = end;
        }
        Ok(Self {
            packet: bytes.clone(),
            time_offset: time_offset.ok_or_else(invalid)?,
        })
    }
    pub(crate) fn refresh(&mut self) -> Result<Option<Stamp>, Failure> {
        self.refresh_at(timestamp()?)
    }
    fn refresh_at(&mut self, time: [u8; 20]) -> Result<Option<Stamp>, Failure> {
        let field = &mut self.packet[self.time_offset..self.time_offset + 20];
        if field == time {
            return Ok(None);
        }
        field.copy_from_slice(&time);
        Ok(Some(Stamp {
            start: 0,
            packet: self.packet.clone(),
            valid_bits: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_fields_describe_actual_build() {
        let bytes = packet(b"Oct 14 2026 07:27:44", b"machine-fingerprint");
        let mut at = 4;
        let mut kinds = Vec::new();
        while at < bytes.len() {
            kinds.push(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
            at += 4 + usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]));
        }
        assert_eq!(kinds, [0x1000, 0x1001, 0x1002, 0x1100, 0x1400]);
        assert_eq!(at, bytes.len());
        for value in [
            env!("DVDA_ENCODER_ID"),
            env!("DVDA_ENCODER_COMPILER"),
            env!("DVDA_ENCODER_TARGET"),
        ] {
            assert!(bytes.windows(value.len()).any(|v| v == value.as_bytes()));
        }
        assert!(env!("DVDA_ENCODER_ID").starts_with("DVD-Audio-Maker "));
        assert!(env!("DVDA_ENCODER_COMPILER").starts_with("rustc "));
    }
    #[test]
    fn machine_fingerprint_is_stable_and_namespaced() {
        let name: Vec<_> = "test-computer".encode_utf16().collect();
        let fingerprint = machine_fingerprint(&name);
        assert_eq!(fingerprint.len(), 64);
        assert_eq!(fingerprint, machine_fingerprint(&name));
        assert_ne!(fingerprint, machine_fingerprint(&[65]));
        assert_eq!(machine_identity().unwrap(), machine_identity().unwrap());
    }
    #[test]
    fn time_matches_c_locale_strftime_and_rejects_invalid_values() {
        let mut time = LocalTime {
            year: 2026,
            month: 1,
            day: 3,
            hour: 2,
            minute: 4,
            second: 5,
            ..Default::default()
        };
        assert_eq!(format_time(&time).unwrap(), *b"Jan 03 2026 02:04:05");
        time.month = 0;
        assert!(format_time(&time).is_err());
    }
    #[cfg(feature = "rust-mlp")]
    #[test]
    fn live_clock_keeps_static_identity_and_handles_second_changes() {
        let original = packet(b"Oct 14 2026 07:27:44", b"machine-id");
        let stamp = Stamp {
            start: 0,
            packet: original.clone(),
            valid_bits: 0,
        };
        let mut live = LiveStamp::new(&stamp).unwrap();
        assert!(live.refresh_at(*b"Oct 14 2026 07:27:44").unwrap().is_none());
        let next = live.refresh_at(*b"Oct 14 2026 07:27:45").unwrap().unwrap();
        assert_eq!(next.packet, packet(b"Oct 14 2026 07:27:45", b"machine-id"));
        assert!(live.refresh_at(*b"Oct 14 2026 07:27:45").unwrap().is_none());
        assert_eq!(
            live.refresh_at(*b"Oct 14 2026 07:27:44")
                .unwrap()
                .unwrap()
                .packet,
            original
        );
        assert!(
            LiveStamp::new(&Stamp {
                start: 0,
                packet: vec![0, 0, 0x40, 0],
                valid_bits: 0
            })
            .is_err()
        );
    }

    #[cfg(feature = "rust-mlp")]
    #[test]
    fn live_clock_rejects_malformed_and_ambiguous_packets() {
        let original = packet(b"Oct 14 2026 07:27:44", b"machine-id");
        let stamp = |packet| Stamp {
            start: 0,
            packet,
            valid_bits: 0,
        };
        for length in 0..original.len() {
            assert!(LiveStamp::new(&stamp(original[..length].to_vec())).is_err());
        }
        let mut duplicate = original.clone();
        field(&mut duplicate, 0x1400, b"Oct 14 2026 07:27:44");
        let length = u16::try_from(duplicate.len() - 4).unwrap().to_be_bytes();
        duplicate[..2].copy_from_slice(&length);
        assert!(LiveStamp::new(&stamp(duplicate)).is_err());
        let mut broken = original.clone();
        broken[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(LiveStamp::new(&stamp(broken)).is_err());
        let mut offset = stamp(original);
        offset.start = 1;
        assert!(LiveStamp::new(&offset).is_err());
    }
}
