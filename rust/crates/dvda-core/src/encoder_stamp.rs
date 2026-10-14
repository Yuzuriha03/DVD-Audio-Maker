//! Application identity for the auxiliary MLP stamp, independent of coding decisions.
use crate::{hash, media::Failure};
use dvda_native::encoder::Stamp;
use std::time::{SystemTime, UNIX_EPOCH};

fn field(packet: &mut Vec<u8>, kind: u16, value: &[u8]) {
    packet.extend_from_slice(
        &u16::try_from(value.len())
            .expect("bounded stamp field")
            .to_be_bytes(),
    );
    packet.extend_from_slice(&kind.to_be_bytes());
    packet.extend_from_slice(value);
}

fn packet(computer: &str, seconds: u64) -> Vec<u8> {
    let machine = hash::hex_digest(
        format!("DVD-Audio-Maker machine v1\0{}", computer.to_uppercase()).as_bytes(),
    );
    let mut payload = Vec::new();
    field(&mut payload, 0x1000, b"DVD-Audio-Maker Rust MLP");
    field(&mut payload, 0x1100, machine.as_bytes());
    field(
        &mut payload,
        0x1400,
        format!("unix-utc:{seconds}").as_bytes(),
    );
    let mut result = Vec::new();
    field(&mut result, 0x4000, &payload);
    result
}

pub(crate) fn current() -> Result<Stamp, Failure> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetComputerNameW(buffer: *mut u16, size: *mut u32) -> i32;
    }
    let mut buffer = [0u16; 256];
    let mut size = buffer.len() as u32;
    // SAFETY: both output pointers remain valid; size is the buffer capacity in UTF-16 units.
    if unsafe { GetComputerNameW(buffer.as_mut_ptr(), &mut size) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let name = String::from_utf16_lossy(&buffer[..size as usize]);
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Failure::new("InvalidData", &error.to_string()))?
        .as_secs();
    Ok(Stamp {
        start: 0,
        packet: packet(&name, seconds),
        valid_bits: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(packet: &[u8]) -> Vec<(u16, &[u8])> {
        let mut rest = packet;
        let mut fields = Vec::new();
        while !rest.is_empty() {
            let length = usize::from(u16::from_be_bytes([rest[0], rest[1]]));
            fields.push((u16::from_be_bytes([rest[2], rest[3]]), &rest[4..4 + length]));
            rest = &rest[4 + length..];
        }
        fields
    }

    #[test]
    fn stamp_preserves_identity_and_time_without_exposing_computer_name() {
        let data = packet("test-host", 1234567890);
        let outer = fields(&data);
        assert_eq!(outer.len(), 1);
        assert_eq!(outer[0].0, 0x4000);
        let items = fields(outer[0].1);
        assert_eq!(
            items.iter().map(|v| v.0).collect::<Vec<_>>(),
            [0x1000, 0x1100, 0x1400]
        );
        assert_eq!(items[1].1.len(), 64);
        assert_eq!(items[2].1, b"unix-utc:1234567890");
        assert!(!String::from_utf8_lossy(&data).contains("test-host"));
        assert_eq!(data, packet("TEST-HOST", 1234567890));
        assert_ne!(data, packet("other-host", 1234567890));
        assert_ne!(data, packet("test-host", 1234567891));
    }
}
