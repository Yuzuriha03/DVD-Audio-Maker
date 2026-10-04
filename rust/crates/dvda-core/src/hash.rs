use serde_json::Value;
use std::io::{self, Read};

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn ch(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ ((!x) & z)
}

fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (x & z) ^ (y & z)
}

fn big_sigma_0(value: u32) -> u32 {
    value.rotate_right(2) ^ value.rotate_right(13) ^ value.rotate_right(22)
}

fn big_sigma_1(value: u32) -> u32 {
    value.rotate_right(6) ^ value.rotate_right(11) ^ value.rotate_right(25)
}

fn small_sigma_0(value: u32) -> u32 {
    value.rotate_right(7) ^ value.rotate_right(18) ^ (value >> 3)
}

fn small_sigma_1(value: u32) -> u32 {
    value.rotate_right(17) ^ value.rotate_right(19) ^ (value >> 10)
}

pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffered: 0,
            length: 0,
        }
    }
}

impl Sha256 {
    pub fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        if self.buffered != 0 {
            let count = (64 - self.buffered).min(data.len());
            self.buffer[self.buffered..self.buffered + count].copy_from_slice(&data[..count]);
            self.buffered += count;
            data = &data[count..];
            if self.buffered != 64 {
                return;
            }
            Self::compress(&mut self.state, &self.buffer);
            self.buffered = 0;
        }
        let (chunks, remainder) = data.as_chunks::<64>();
        for chunk in chunks {
            Self::compress(&mut self.state, chunk);
        }
        self.buffer[..remainder.len()].copy_from_slice(remainder);
        self.buffered = remainder.len();
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bit_length = self.length.wrapping_mul(8);
        self.buffer[self.buffered] = 0x80;
        self.buffer[self.buffered + 1..].fill(0);
        if self.buffered >= 56 {
            Self::compress(&mut self.state, &self.buffer);
            self.buffer.fill(0);
        }
        self.buffer[56..].copy_from_slice(&bit_length.to_be_bytes());
        Self::compress(&mut self.state, &self.buffer);
        let mut digest = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    fn compress(state: &mut [u32; 8], chunk: &[u8]) {
        let mut words = [0u32; 64];
        for (index, word) in words[..16].iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            words[index] = small_sigma_1(words[index - 2])
                .wrapping_add(words[index - 7])
                .wrapping_add(small_sigma_0(words[index - 15]))
                .wrapping_add(words[index - 16]);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for index in 0..64 {
            let t1 = h
                .wrapping_add(big_sigma_1(e))
                .wrapping_add(ch(e, f, g))
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let t2 = big_sigma_0(a).wrapping_add(maj(a, b, c));
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
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
