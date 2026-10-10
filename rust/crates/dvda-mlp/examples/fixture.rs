use dvda_mlp::{Config, encode};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rate = args[1].parse().unwrap();
    let bits: u8 = args[2].parse().unwrap();
    let channels: u8 = args[3].parse().unwrap();
    let frames: usize = args[4].parse().unwrap();
    let mut pcm = Vec::new();
    let mut raw = Vec::new();
    for n in 0..frames {
        for ch in 0..channels {
            let value = (((n as i32 * 7919 + i32::from(ch) * 104729) % (1 << bits))
                - (1 << (bits - 1)))
                << (24 - bits);
            pcm.push(value);
            raw.extend((value << 8).to_le_bytes());
        }
    }
    let config = Config {
        sample_rate: rate,
        bits,
        channels,
        restart_interval: 8,
        metadata: vec![0, 0, 0x40, 0],
    };
    std::fs::write(&args[5], encode(&config, &pcm, || false).unwrap()).unwrap();
    std::fs::write(&args[6], raw).unwrap();
}
