use super::{Sha256, hex, reader_digest, sha256};
use sha2::Digest;
use std::{fs, hint::black_box, path::Path, time::Instant};

#[test]
fn batch_matches_sha2_for_padding_and_chunk_boundaries() {
    let mut seed = 0x9e3779b97f4a7c15u64;
    for size in (0..=260).chain([
        511, 512, 513, 1023, 4096, 65535, 65536, 65537, 131071, 131072, 131073,
    ]) {
        let data: Vec<u8> = (0..size)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        let expected: [u8; 32] = sha2::Sha256::digest(&data).into();
        assert_eq!(sha256(&data), expected, "size={size}");
        assert_eq!(reader_digest(data.as_slice()).unwrap(), hex(&expected));
        for chunk_size in [1, 7, 55, 56, 63, 64, 65, 127, 4096, 65537, 131072] {
            let mut new = Sha256::default();
            new.update(&[]);
            for chunk in data.chunks(chunk_size) {
                new.update(chunk);
                new.update(&[]);
            }
            assert_eq!(new.finish(), expected, "size={size}, chunk={chunk_size}");
        }
    }
}

fn digest_file(path: &Path) -> std::io::Result<String> {
    reader_digest(fs::File::open(path)?)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

#[test]
#[ignore = "read-only Release benchmark; requires DVDA_HASH_BENCH_INPUTS and DVDA_HASH_BENCH_REPORT"]
fn release_files_benchmark() {
    let inputs: Vec<std::path::PathBuf> = serde_json::from_str(
        &std::env::var("DVDA_HASH_BENCH_INPUTS").expect("JSON array of real file paths"),
    )
    .unwrap();
    assert!(!inputs.is_empty());
    let report = std::env::var_os("DVDA_HASH_BENCH_REPORT").expect("Report destination");
    assert!(
        !Path::new(&report).exists(),
        "Report must not overwrite an existing file"
    );
    assert!(!inputs.iter().any(|path| path == Path::new(&report)));
    let mut results = Vec::new();
    for path in inputs {
        let bytes = fs::read(&path).unwrap();
        assert!(
            !bytes.is_empty(),
            "Empty benchmark input: {}",
            path.display()
        );
        let expected = hex(&sha256(&bytes));
        assert_eq!(digest_file(&path).unwrap(), expected);
        let repetitions = (64 * 1024 * 1024 / bytes.len()).clamp(1, 4096);
        let mut rows = Vec::new();
        for mode in ["memory", "warm_file"] {
            let mut samples = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                for _ in 0..repetitions {
                    let digest = if mode == "warm_file" {
                        digest_file(&path).unwrap()
                    } else {
                        hex(&sha256(black_box(&bytes)))
                    };
                    assert_eq!(black_box(digest), expected);
                }
                samples.push(start.elapsed().as_secs_f64() * 1000.0 / repetitions as f64);
            }
            let sha2_median = median(&mut samples);
            let mib = bytes.len() as f64 / (1024.0 * 1024.0);
            rows.push(serde_json::json!({
                "mode": mode, "repetitions_per_round": repetitions, "rounds": 7,
                "sha2_ms": sha2_median,
                "sha2_mib_s": mib * 1000.0 / sha2_median,
                "sha2_samples_ms": samples
            }));
        }
        assert_eq!(
            digest_file(&path).unwrap(),
            expected,
            "Input changed during benchmark"
        );
        results.push(serde_json::json!({"path": path, "bytes": bytes.len(), "sha256": expected, "results": rows}));
    }
    #[cfg(target_arch = "x86_64")]
    let cpu_features = serde_json::json!({
        "sha": std::is_x86_feature_detected!("sha"),
        "sse2": std::is_x86_feature_detected!("sse2"),
        "ssse3": std::is_x86_feature_detected!("ssse3"),
        "sse4.1": std::is_x86_feature_detected!("sse4.1")
    });
    #[cfg(not(target_arch = "x86_64"))]
    let cpu_features = serde_json::Value::Null;
    let output = serde_json::json!({
        "backend": "sha2 0.11.0 default runtime dispatch", "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS, "cpu_features": cpu_features,
        "method": "Release; sha2 only; seven-round medians; warm OS file cache; inputs read-only; no cold-cache claims",
        "files": results
    });
    fs::write(report, serde_json::to_vec_pretty(&output).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
