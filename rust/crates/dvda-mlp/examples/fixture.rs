use dvda_mlp::{
    Config,
    direct::{StreamIo, encode_stream},
    format::Profile,
    metadata::Metadata,
};

struct PcmIo {
    pcm: Vec<i32>,
    channels: usize,
    position: usize,
    output: Vec<u8>,
}

impl StreamIo for PcmIo {
    fn read(&mut self, _group: usize, pcm: &mut [i32]) -> Result<usize, i32> {
        let remaining = self.pcm.len() / self.channels - self.position;
        let count = remaining.min(pcm.len() / self.channels);
        let start = self.position * self.channels;
        pcm[..count * self.channels]
            .copy_from_slice(&self.pcm[start..start + count * self.channels]);
        self.position += count;
        Ok(count)
    }

    fn write(&mut self, access_unit: &[u8]) -> Result<(), i32> {
        self.output.extend_from_slice(access_unit);
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rate = args[1].parse().unwrap();
    let bits: u8 = args[2].parse().unwrap();
    let channels: u8 = args[3].parse().unwrap();
    let frames: usize = args[4].parse().unwrap();
    let mut pcm = Vec::new();
    let mut raw = Vec::new();
    for n in 0..frames {
        let n = i32::try_from(n).expect("fixture frame index must fit i32");
        for ch in 0..channels {
            let value = (((n * 7919 + i32::from(ch) * 104_729) % (1 << bits)) - (1 << (bits - 1)))
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
    let profile = Profile::standard(&config);
    let metadata = [Metadata {
        start: 0,
        packet: config.metadata.clone(),
        valid_bits: 0,
    }];
    let mut io = PcmIo {
        pcm,
        channels: usize::from(channels),
        position: 0,
        output: Vec::new(),
    };
    let result = encode_stream(
        &config,
        &profile,
        u64::try_from(frames).expect("fixture frame count fits u64"),
        None,
        &metadata,
        &mut io,
    );
    let error: Vec<u8> = result
        .error
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte.to_ne_bytes()[0])
        .collect();
    assert_eq!(
        result.status,
        0,
        "fixture encoding failed: {}",
        String::from_utf8_lossy(&error)
    );
    std::fs::write(&args[5], io.output).unwrap();
    std::fs::write(&args[6], raw).unwrap();
}
