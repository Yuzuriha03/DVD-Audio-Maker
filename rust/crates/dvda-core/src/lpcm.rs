use serde_json::{Value, json};

fn integer(request: &Value, key: &str) -> Result<i64, String> {
    request[key]
        .as_i64()
        .ok_or_else(|| format!("Missing {key}"))
}

pub fn format_code(rate: i64, bits: i64, channels: i64) -> i64 {
    if !matches!(rate, 44100 | 48000 | 88200 | 96000 | 176400 | 192000)
        || !matches!(bits, 16 | 24)
        || !(1..=6).contains(&channels)
        || (rate > 96000 && channels > 2)
    {
        return 1;
    }
    if rate.saturating_mul(bits).saturating_mul(channels) > 9_600_000 {
        return 2;
    }
    0
}

pub fn layout_code(rate: i64, bits: i64, channels: i64, mask: i64) -> i64 {
    let format = format_code(rate, bits, channels);
    if format != 0 {
        return format;
    }
    if matches!(
        mask,
        4 | 3 | 0x103 | 0x33 | 0xb | 0x10b | 0x3b | 7 | 0x107 | 0x37 | 0xf | 0x10f | 0x3f
    ) {
        0
    } else {
        3
    }
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    let rate = integer(&request, "Rate")?;
    let bits = integer(&request, "Bits")?;
    let channels = integer(&request, "Channels")?;
    let code = match operation {
        "lpcm.validate_format" => format_code(rate, bits, channels),
        "lpcm.validate_layout" => {
            layout_code(rate, bits, channels, integer(&request, "ChannelMask")?)
        }
        _ => return Err(format!("Unsupported Rust LPCM operation: {operation}")),
    };
    Ok(json!(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_dvd_audio_format_matrix_and_bitrate_limit() {
        assert_eq!(format_code(48000, 24, 6), 0);
        assert_eq!(format_code(192000, 24, 3), 1);
        assert_eq!(format_code(96000, 24, 6), 2);
    }

    #[test]
    fn rejects_unknown_channel_masks() {
        assert_eq!(layout_code(48000, 24, 2, 0x1234), 3);
        assert_eq!(layout_code(48000, 24, 2, 3), 0);
    }
}
