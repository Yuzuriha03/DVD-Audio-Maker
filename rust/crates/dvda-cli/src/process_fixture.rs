//! Development-only child used by the independent process contract tests.
use std::{
    io::{self, Write},
    os::windows::process::CommandExt,
    process::Command,
    time::{Duration, Instant},
};
pub fn run(args: &[String]) -> io::Result<i32> {
    match args.first().map(String::as_str) {
        Some("echo") => {
            println!("{}", serde_json::to_string(&args[1..]).unwrap());
            eprintln!("{}", std::env::current_dir()?.display());
            eprintln!(
                "{}",
                std::env::var("DVDA_PROCESS_VALUE").unwrap_or_else(|_| "missing".into())
            );
            eprintln!(
                "{}",
                std::env::var("DVDA_PROCESS_REMOVE").unwrap_or_else(|_| "missing".into())
            );
        }
        Some("lines") => {
            let text = "中文-日本語-🎵\r\n\nalpha\rbravo\nlast\0tail";
            let code: u32 = args[1].parse().unwrap();
            let encode = |text: &str| -> Vec<u8> {
                match code {
                    1200 => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                    1201 => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
                    12000 => text
                        .chars()
                        .flat_map(|c| (c as u32).to_le_bytes())
                        .collect(),
                    12001 => text
                        .chars()
                        .flat_map(|c| (c as u32).to_be_bytes())
                        .collect(),
                    _ => text.as_bytes().to_vec(),
                }
            };
            let mut output = io::stdout().lock();
            for b in encode(text) {
                output.write_all(&[b])?;
                output.flush()?;
            }
            io::stderr().write_all(&encode("error\r\nlast"))?;
        }
        Some("burst") => {
            for i in 0..2048 {
                println!("out-{i:05}{}", "x".repeat(80));
                eprintln!("err-{i:05}{}", "y".repeat(80));
            }
            return Ok(7);
        }
        Some(kind @ ("sleep" | "tree")) => {
            std::fs::write(&args[1], std::process::id().to_string())?;
            let mut child = if kind == "tree" {
                Some(
                    Command::new(std::env::current_exe()?)
                        .args(["process-fixture", "sleep", &args[2]])
                        .creation_flags(0x08000000)
                        .spawn()?,
                )
            } else {
                None
            };
            if child.is_some() {
                let start = Instant::now();
                while !std::path::Path::new(&args[2]).exists()
                    && start.elapsed() < Duration::from_secs(10)
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                if !std::path::Path::new(&args[2]).exists() {
                    return Err(io::Error::other("Child fixture failed to start"));
                }
            }
            println!("ready");
            io::stdout().flush()?;
            std::thread::sleep(Duration::from_secs(30));
            if let Some(child) = &mut child {
                let _ = child.wait();
            }
        }
        _ => return Err(io::Error::other("Unknown process fixture")),
    }
    Ok(0)
}
