//! Developer-only streaming verifier for authored AOB fixtures.
use std::{fs::File, io::Read, path::PathBuf};

pub fn run(args: &[String]) -> Result<(), String> {
    let mut sources = Vec::new();
    let mut aobs = Vec::new();
    let mut ends = None;
    let mut lpcm = false;
    let mut args = args.iter();
    while let Some(option) = args.next() {
        match option.as_str() {
            "--lpcm" => lpcm = true,
            "--source" | "--aob" | "--title-ends" => {
                let value = args
                    .next()
                    .filter(|value| !value.is_empty() && !value.starts_with("--"))
                    .ok_or_else(|| format!("{option} requires a value"))?;
                match option.as_str() {
                    "--source" => sources.push(PathBuf::from(value)),
                    "--aob" => aobs.push(PathBuf::from(value)),
                    _ => {
                        let values = value
                            .split(',')
                            .map(|value| match value {
                                "0" => Ok(0),
                                "1" => Ok(1),
                                _ => Err("Title ends must be comma-separated 0 or 1"),
                            })
                            .collect::<Result<Vec<u8>, _>>()?;
                        ends = Some(values);
                    }
                }
            }
            _ => return Err(format!("Unknown verify-aob option: {option}")),
        }
    }
    if sources.is_empty() || aobs.is_empty() || lpcm != ends.is_some() {
        return Err(
            "verify-aob needs --source and --aob; LPCM also needs --lpcm --title-ends".into(),
        );
    }
    let mut at = 0;
    let mut current: Option<File> = None;
    let chunks = std::iter::from_fn(|| {
        loop {
            if current.is_none() {
                if at == aobs.len() {
                    return None;
                }
                match File::open(&aobs[at]) {
                    Ok(file) => current = Some(file),
                    Err(e) => {
                        at += 1;
                        return Some(Err(e.to_string()));
                    }
                }
                at += 1;
            }
            let mut data = vec![0; 128 * 1024];
            let mut filled = 0;
            while filled < data.len() {
                match current.as_mut().unwrap().read(&mut data[filled..]) {
                    Ok(0) => break,
                    Ok(length) => filled += length,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Some(Err(error.to_string())),
                }
            }
            if filled == 0 {
                current = None;
            } else {
                data.truncate(filled);
                return Some(Ok(data));
            }
        }
    });
    let verifier = dvda_native::disc_verify::NativeDiscVerifier::load()?;
    let result = if let Some(ends) = ends {
        verifier.verify_lpcm(&sources, &ends, chunks)?
    } else {
        verifier.verify_mlp(&sources, chunks)?
    };
    println!(
        "{}",
        serde_json::json!({"bytes":result.bytes,"sectors":result.sectors,"track":result.track,"offset":result.offset})
    );
    Ok(())
}
