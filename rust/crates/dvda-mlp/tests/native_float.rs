//! Opt-in independent decoder validation; requires ffmpeg on PATH.
use dvda_mlp::{
    direct::{encode_stream, StreamIo},
    format::Profile,
    metadata::Metadata,
    Config,
};
use std::{process::Command, time::Instant};

struct Io {
    pcm: Vec<i32>,
    position: usize,
    channels: usize,
    output: Vec<u8>,
}
impl StreamIo for Io {
    fn read(&mut self, _: usize, pcm: &mut [i32]) -> Result<usize, i32> {
        let count = pcm.len().min(self.pcm.len() - self.position);
        pcm[..count].copy_from_slice(&self.pcm[self.position..self.position + count]);
        self.position += count;
        Ok(count / self.channels)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<(), i32> {
        self.output.extend_from_slice(bytes);
        Ok(())
    }
}

#[test]
fn long_stream_reports_fifo_rate_limit() {
    let frames = 64001;
    let config = Config {
        sample_rate: 88200,
        bits: 20,
        channels: 6,
        restart_interval: 8,
        metadata: vec![0, 0, 0x40, 0],
    };
    let records = [Metadata {
        start: 0,
        packet: config.metadata.clone(),
        valid_bits: 0,
    }];
    let mut rng = 0x12345678u32;
    let pcm = (0..frames * 6)
        .map(|_| {
            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            (((rng >> 12) as i32) - (1 << 19)) << 4
        })
        .collect();
    let mut io = Io { pcm, position: 0, channels: 6, output: Vec::new() };
    let result = encode_stream(
        &config, &Profile::standard(&config), frames as u64, None, &records, &mut io,
    );
    assert_eq!(result.status, -5);
    let error: Vec<u8> = result.error.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    assert_eq!(String::from_utf8(error).unwrap(),
        "MLP FIFO rate limit exceeded; input cannot be encoded at the required delivery rate");
    assert!(result.input_frames > 0);
    assert_eq!(result.output_bytes, io.output.len() as u64);
    assert_eq!(result.encoded_frames, 0);
    assert!(result.error.last() == Some(&0));

    // The format itself is valid: compressible PCM at the same settings succeeds.
    io.pcm.fill(0);
    io.position = 0;
    io.output.clear();
    let result = encode_stream(
        &config, &Profile::standard(&config), frames as u64, None, &records, &mut io,
    );
    assert_eq!(result.status, 0);
    assert_eq!(result.encoded_frames, 64080);
    assert!(result.error.iter().all(|&c| c == 0));
}

#[test]
#[ignore = "requires ffmpeg; run with --ignored --nocapture"]
fn native_float_lossless_matrix() {
    let root = std::env::temp_dir().join(format!("dvda-float-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut report = String::from("rate,bits,channels,pattern,bytes,encode_us\n");
    for rate in [44100, 48000, 88200, 96000, 176400, 192000] {
        for bits in [16, 20, 24] {
            for channels in [1, 2, 6] {
                if rate > 96000 && channels > 2 {
                    continue;
                }
                for pattern in 0..4 {
                    let frames = std::env::var("DVDA_FLOAT_FRAMES")
                        .map(|value| value.parse::<usize>().expect("DVDA_FLOAT_FRAMES must be an integer"))
                        .unwrap_or(1281);
                    assert!(frames > 0);
                    let mut rng = 0x12345678u32;
                    let mut pcm = Vec::new();
                    for n in 0..frames {
                        for ch in 0..channels {
                            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                            let max = (1i32 << (bits - 1)) - 1;
                            let value = match pattern {
                                0 => 0,
                                1 => {
                                    if (n + ch) % 2 == 0 {
                                        max
                                    } else {
                                        -max - 1
                                    }
                                }
                                2 => ((n * 7919 + ch * 17) as i32 & max) - max / 2,
                                _ => ((rng >> (32 - bits)) as i32) - (1 << (bits - 1)),
                            };
                            pcm.push(value << (24 - bits));
                        }
                    }
                    let config = Config {
                        sample_rate: rate,
                        bits,
                        channels: channels as u8,
                        restart_interval: 8,
                        metadata: vec![0, 0, 0x40, 0],
                    };
                    let records = [Metadata {
                        start: 0,
                        packet: config.metadata.clone(),
                        valid_bits: 0,
                    }];
                    let mut io = Io {
                        pcm,
                        position: 0,
                        channels,
                        output: Vec::new(),
                    };
                    let start = Instant::now();
                    let result = encode_stream(
                        &config,
                        &Profile::standard(&config),
                        frames as u64,
                        None,
                        &records,
                        &mut io,
                    );
                    let elapsed = start.elapsed().as_micros();
                    assert_eq!(result.status, 0, "{rate}/{bits}/{channels}/{pattern}");
                    let input = root.join("input.mlp");
                    let output = root.join("decoded.pcm");
                    std::fs::write(&input, &io.output).unwrap();
                    let decoded = Command::new("ffmpeg")
                        .args(["-v", "error", "-xerror", "-y", "-f", "mlp", "-i"])
                        .arg(&input)
                        .args(["-f", "s32le", "-acodec", "pcm_s32le"])
                        .arg(&output)
                        .output()
                        .unwrap();
                    assert!(
                        decoded.status.success(),
                        "{}",
                        String::from_utf8_lossy(&decoded.stderr)
                    );
                    let raw = std::fs::read(&output).unwrap();
                    let expected: Vec<u8> =
                        io.pcm.iter().flat_map(|x| (x << 8).to_le_bytes()).collect();
                    assert!(raw.len() >= expected.len());
                    assert_eq!(
                        &raw[..expected.len()],
                        &expected,
                        "{rate}/{bits}/{channels}/{pattern}"
                    );
                    assert!(raw[expected.len()..].iter().all(|&x| x == 0));
                    assert_eq!(raw.len(), result.encoded_frames as usize * channels * 4);
                    report.push_str(&format!(
                        "{rate},{bits},{channels},{pattern},{},{elapsed}\n",
                        io.output.len()
                    ));
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("DVDA_FLOAT_REPORT") {
        std::fs::write(path, &report).unwrap();
    }
    println!("Validated {} lossless cases", report.lines().count() - 1);
    std::fs::remove_dir_all(root).unwrap();
}
