use serde_json::Value;
use std::io::{self, Read};

use sha2::Digest;

#[cfg(test)]
mod regression;

#[derive(Default)]
pub struct Sha256(sha2::Sha256);

impl Sha256 {
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    pub fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::default();
    hash.update(data);
    hash.finish()
}

pub fn hex_digest(data: &[u8]) -> String {
    hex(&sha256(data))
}

pub fn reader_digest(mut reader: impl Read) -> io::Result<String> {
    let mut hash = Sha256::default();
    let mut buffer = vec![0; 128 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex(&hash.finish()))
}

fn hex(digest: &[u8; 32]) -> String {
    let mut result = String::with_capacity(64);
    for byte in digest {
        result.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
        result.push(char::from(b"0123456789ABCDEF"[(byte & 0x0f) as usize]));
    }
    result
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "hash.sha256_hex" => Ok(Value::String(hex_digest(
            request
                .as_str()
                .ok_or("Expected UTF-8 text to hash")?
                .as_bytes(),
        ))),
        "hash.sha256_file" => {
            let path = request.as_str().ok_or("Expected file path")?;
            let result = crate::identity::open_read(path).and_then(reader_digest);
            Ok(match result {
                Ok(hash) => serde_json::json!({"Hash": hash, "ErrorCode": null, "Error": null}),
                Err(error) => {
                    serde_json::json!({"Hash": null, "ErrorCode": error.raw_os_error(), "Error": error.to_string()})
                }
            })
        }
        _ => Err(format!("Unsupported Rust hash operation: {operation}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_standard_sha256_vectors() {
        assert_eq!(
            hex_digest(b""),
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        );
        assert_eq!(
            hex_digest(b"abc"),
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        );
    }

    #[test]
    fn incremental_hash_matches_million_a_vector() {
        let data = vec![b'a'; 1_000_000];
        for chunk_size in [1, 55, 56, 63, 64, 65, 65537] {
            let mut hash = Sha256::default();
            for chunk in data.chunks(chunk_size) {
                hash.update(chunk);
                hash.update(&[]);
            }
            assert_eq!(
                hex(&hash.finish()),
                "CDC76E5C9914FB9281A1C7E284D73E67F1809A48A497200E046D39CCC7112CD0"
            );
        }
    }
}
