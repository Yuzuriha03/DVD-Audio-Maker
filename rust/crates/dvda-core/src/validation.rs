use serde_json::{Value, json};

fn value<'a>(object: &'a Value, key: &str) -> Option<&'a Value> {
    object.as_object().and_then(|map| {
        map.get(key)
            .or_else(|| map.get(&key.to_ascii_lowercase()))
            .or_else(|| map.get(snake_case(key).as_str()))
    })
}

fn snake_case(key: &str) -> String {
    let mut result = String::with_capacity(key.len() + 4);
    for (index, character) in key.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

fn text<'a>(object: &'a Value, key: &str) -> Option<&'a str> {
    value(object, key).and_then(Value::as_str)
}

fn hex_value(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        b'A'..=b'F' => Some(character - b'A' + 10),
        _ => None,
    }
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut result = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        result.push((high << 4) | low);
    }
    Some(result)
}

/// Returns the distance, in access units, between the first two major-sync markers.
/// A malformed or incomplete access-unit stream has no usable interval.
pub fn major_sync_interval(data: &[u8]) -> Option<i64> {
    let mut position = 0usize;
    let mut access_unit = 0i64;
    let mut first = None;
    while position + 4 <= data.len() {
        let header = u16::from_be_bytes([data[position], data[position + 1]]);
        let length = ((header & 0x0fff) as usize).saturating_mul(2);
        let end = position.checked_add(length)?;
        if length < 4 || end > data.len() {
            return None;
        }
        if position + 7 <= data.len() && data[position + 4..position + 7] == [0xf8, 0x72, 0x6f] {
            if let Some(first_unit) = first {
                return Some(access_unit - first_unit);
            }
            first = Some(access_unit);
        }
        position = end;
        access_unit += 1;
    }
    None
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "mlp.major_sync_interval" => {
            let encoded = request
                .as_str()
                .or_else(|| text(&request, "DataHex"))
                .or_else(|| text(&request, "Hex"));
            let result = encoded
                .and_then(decode_hex)
                .and_then(|data| major_sync_interval(&data));
            Ok(json!(result))
        }
        _ => Err(format!(
            "Unsupported Rust validation operation: {operation}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut data = vec![0u8; 24];
        for position in [0usize, 8, 16] {
            data[position..position + 2].copy_from_slice(&4u16.to_be_bytes());
        }
        data[4..7].copy_from_slice(&[0xf8, 0x72, 0x6f]);
        data[20..23].copy_from_slice(&[0xf8, 0x72, 0x6f]);
        data
    }

    #[test]
    fn finds_interval_between_major_sync_markers() {
        assert_eq!(major_sync_interval(&fixture()), Some(2));
    }

    #[test]
    fn rejects_incomplete_access_units() {
        let mut data = fixture();
        data[0..2].copy_from_slice(&20u16.to_be_bytes());
        assert_eq!(major_sync_interval(&data), None);
    }

    #[test]
    fn dispatch_accepts_hex_request_and_invalid_hex_is_null() {
        let encoded = fixture()
            .iter()
            .map(|value| format!("{value:02X}"))
            .collect::<String>();
        assert_eq!(
            dispatch("mlp.major_sync_interval", json!({"DataHex": encoded})).unwrap(),
            json!(2)
        );
        assert_eq!(
            dispatch("mlp.major_sync_interval", json!("GG")).unwrap(),
            Value::Null
        );
    }
}
