//! Bounded pipe reads, line events and cancellation for the source-built author.
use crate::media::{Failure, Timed};
use dvda_native::{
    media::Callbacks,
    process::{self, Child, Launch, ReadState},
};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Instant};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub file_name: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    pub environment: Vec<(String, Option<String>)>,
    pub output_code_page: u32,
    pub error_code_page: u32,
    pub capture_output: bool,
    pub capture_error: bool,
    pub timeout_millis: Option<u64>,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub failure: Option<Failure>,
    pub standard_output: String,
    pub standard_error: String,
    pub duration_seconds: f64,
}
struct Lines {
    code_page: u32,
    width: usize,
    pending: Vec<u8>,
    line: Vec<u8>,
    skip_lf: bool,
}
impl Lines {
    fn new(code_page: u32) -> Self {
        Self {
            code_page,
            width: match code_page {
                1200 | 1201 => 2,
                12000 | 12001 => 4,
                _ => 1,
            },
            pending: Vec::new(),
            line: Vec::new(),
            skip_lf: false,
        }
    }
    fn feed(&mut self, bytes: &[u8], end: bool, mut emit: impl FnMut(&str)) -> Result<(), Failure> {
        self.pending.extend_from_slice(bytes);
        let available = self.pending.len() / self.width * self.width;
        for unit in self.pending[..available].chunks_exact(self.width) {
            let code = match self.code_page {
                1200 => u32::from(u16::from_le_bytes(unit.try_into().unwrap())),
                1201 => u32::from(u16::from_be_bytes(unit.try_into().unwrap())),
                12000 => u32::from_le_bytes(unit.try_into().unwrap()),
                12001 => u32::from_be_bytes(unit.try_into().unwrap()),
                _ => u32::from(unit[0]),
            };
            if code == 10 && self.skip_lf {
                self.skip_lf = false;
                continue;
            }
            self.skip_lf = code == 13;
            if code == 10 || code == 13 {
                emit(&process::decode(&self.line, self.code_page)?);
                self.line.clear();
            } else {
                self.line.extend_from_slice(unit);
            }
        }
        self.pending.drain(..available);
        if end {
            self.line.append(&mut self.pending);
            if !self.line.is_empty() {
                emit(&process::decode(&self.line, self.code_page)?);
                self.line.clear();
            }
        }
        Ok(())
    }
}
pub fn execute(job: Job, caller: &mut dyn Callbacks) -> Outcome {
    let start = Instant::now();
    let mut callbacks = Timed::new(caller, job.timeout_millis);
    let mut captured = [String::new(), String::new()];
    let result = (|| -> Result<i32, Failure> {
        if crate::config::trim(&job.file_name).is_empty() {
            return Err(Failure::new("Argument", "Process executable is empty"));
        }
        callbacks.check()?;
        let mut child = Child::spawn(Launch {
            file: &job.file_name,
            arguments: &job.arguments,
            directory: job
                .working_directory
                .as_deref()
                .filter(|p| !crate::config::trim(p).is_empty())
                .map(Path::new),
            environment: &job.environment,
        })
        .map_err(|error| {
            Failure::new(
                "InvalidOperation",
                &format!("无法启动外部程序 {}: {error}", job.file_name),
            )
        })?;
        let mut lines = [
            Lines::new(job.output_code_page),
            Lines::new(job.error_code_page),
        ];
        let mut closed = [false; 2];
        let mut buffer = [0u8; 8192];
        loop {
            callbacks.check()?;
            let mut read_any = false;
            for stream in 0..2 {
                if closed[stream] {
                    continue;
                }
                let mut emit = |line: &str| {
                    if [job.capture_output, job.capture_error][stream] {
                        captured[stream].push_str(line);
                        captured[stream].push_str("\r\n");
                    }
                    callbacks.emit(stream as i32 + 1, line);
                };
                match child.read(stream, &mut buffer)? {
                    ReadState::Data(count) => {
                        read_any = true;
                        lines[stream].feed(&buffer[..count], false, &mut emit)?;
                    }
                    ReadState::Closed => {
                        closed[stream] = true;
                        lines[stream].feed(&[], true, &mut emit)?;
                    }
                    ReadState::Empty => (),
                }
            }
            if let Some(code) = child.exit_code()? {
                if closed.iter().all(|v| *v) {
                    callbacks.check()?;
                    return Ok(code);
                }
                if !read_any {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            } else if !read_any {
                child.wait_briefly();
            }
        }
    })();
    let [standard_output, standard_error] = captured;
    match result {
        Ok(code) => Outcome {
            exit_code: Some(code),
            failure: None,
            standard_output,
            standard_error,
            duration_seconds: start.elapsed().as_secs_f64(),
        },
        Err(error) => Outcome {
            exit_code: None,
            failure: Some(error),
            standard_output,
            standard_error,
            duration_seconds: start.elapsed().as_secs_f64(),
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_lines_and_encoding() {
        for code_page in [65001, 1200, 1201, 12000, 12001] {
            let text = "中文🎵\r\n\nalpha\rbravo\nlast";
            let bytes: Vec<u8> = match code_page {
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
            };
            let mut decoder = Lines::new(code_page);
            let mut lines = Vec::new();
            for b in bytes {
                decoder
                    .feed(&[b], false, |line| lines.push(line.to_owned()))
                    .unwrap();
            }
            decoder
                .feed(&[], true, |line| lines.push(line.to_owned()))
                .unwrap();
            assert_eq!(lines, ["中文🎵", "", "alpha", "bravo", "last"]);
        }
    }
}
