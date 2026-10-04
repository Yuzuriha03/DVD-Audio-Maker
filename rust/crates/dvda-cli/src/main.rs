use std::{io::Read, path::Path};
mod process_fixture;
fn main() {
    let op = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "abi.version".into());
    if op == "process-fixture" {
        let args: Vec<_> = std::env::args().skip(2).collect();
        let code = process_fixture::run(&args).unwrap_or_else(|e| {
            eprintln!("{e}");
            1
        });
        std::process::exit(code);
    }
    if op == "formats-sample" {
        let root = std::env::args().nth(2).unwrap_or_else(|| {
            eprintln!("Usage: dvda-cli formats-sample <MLP root>");
            std::process::exit(2);
        });
        if let Err(error) = formats_sample(Path::new(&root)) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    let mut input = String::new();
    if op != "abi.version" {
        std::io::stdin()
            .read_to_string(&mut input)
            .expect("Read JSON input");
    }
    let result = serde_json::from_str(if input.is_empty() { "null" } else { &input })
        .map_err(|e| e.to_string())
        .and_then(|v| dvda_core::dispatch(&op, v));
    match result {
        Ok(v) => println!("{v}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn formats_sample(root: &Path) -> Result<(), String> {
    let native = dvda_native::NativeFormats::load()?;
    let mut paths = Vec::new();
    collect_mlp(root, &mut paths)?;
    if paths.is_empty() {
        return Err("No MLP samples were found".into());
    }
    paths.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    let mut bytes = 0u64;
    let mut aligned = 0usize;
    let mut valid = 0usize;
    for path in &paths {
        let inspection = native.inspect_file(path)?;
        if inspection.is_valid {
            valid += 1;
        }
        bytes = bytes
            .checked_add(inspection.size)
            .ok_or("Sample byte count overflow")?;
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let result = native.align(&data)?;
        if result.data.is_empty() && !data.is_empty() {
            return Err(format!("Empty alignment result: {}", path.display()));
        }
        aligned += 1;
    }
    let pts: Vec<i64> = dvda_native::sample_pts()
        .into_iter()
        .map(|value| {
            let encoded = encode_pts(value);
            native.parse_pts(&encoded)
        })
        .collect::<Result<_, _>>()?;
    println!(
        "native Rust/C17 samples: {}/{} MLP files, {} bytes, {} aligned, {} valid, PTS={:?}",
        paths.len(),
        paths.len(),
        bytes,
        aligned,
        valid,
        pts
    );
    Ok(())
}

fn collect_mlp(root: &Path, output: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_mlp(&path, output)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mlp"))
        {
            output.push(path);
        }
    }
    Ok(())
}

fn encode_pts(value: i64) -> [u8; 5] {
    [
        0x21 | (((value >> 29) & 0x0e) as u8),
        (value >> 22) as u8,
        (((value >> 14) & 0xfe) as u8) | 1,
        (value >> 7) as u8,
        (((value << 1) & 0xfe) as u8) | 1,
    ]
}
