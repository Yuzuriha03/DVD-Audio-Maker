//! In-process media jobs. The final Rust application uses this typed API directly.
use dvda_native::{
    files,
    media::{Callbacks, Media, Request},
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub library: PathBuf,
    pub request: Request,
    pub replace: bool,
    pub timeout_millis: Option<u64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Failure {
    pub kind: &'static str,
    pub code: Option<i32>,
    pub message: String,
}
impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self {
            kind: "Io",
            code: error.raw_os_error(),
            message: error.to_string(),
        }
    }
}
impl Failure {
    pub(crate) fn new(kind: &'static str, message: &str) -> Self {
        Self {
            kind,
            code: None,
            message: message.into(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub failure: Option<Failure>,
}

pub(crate) struct Timed<'a> {
    caller: &'a mut dyn Callbacks,
    start: Instant,
    timeout: Option<Duration>,
    external: bool,
    expired: bool,
}
impl Callbacks for Timed<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        self.caller.emit(stream, text);
    }
    fn cancelled(&mut self) -> bool {
        self.external |= self.caller.cancelled();
        self.expired |= self
            .timeout
            .is_some_and(|timeout| self.start.elapsed() >= timeout);
        self.external || self.expired
    }
}
impl Timed<'_> {
    pub(crate) fn new(caller: &mut dyn Callbacks, timeout_millis: Option<u64>) -> Timed<'_> {
        Timed {
            caller,
            start: Instant::now(),
            timeout: timeout_millis.map(Duration::from_millis),
            external: false,
            expired: false,
        }
    }
    pub(crate) fn check(&mut self) -> Result<(), Failure> {
        self.cancelled();
        if self.external {
            Err(Failure::new("Cancelled", "Media job cancelled"))
        } else if self.expired {
            Err(Failure::new("Timeout", "Media job timed out"))
        } else {
            Ok(())
        }
    }
}
pub(crate) struct Temporary(pub(crate) Option<PathBuf>);
impl Temporary {
    pub(crate) fn clean(&mut self) -> io::Result<()> {
        if let Some(path) = &self.0 {
            if path.is_file() {
                std::fs::remove_file(path)?;
            }
            self.0 = None;
        }
        Ok(())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = self.clean();
    }
}
pub(crate) fn temporary_path(destination: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut path = destination.as_os_str().to_owned();
    path.push(format!(
        ".{}-{}-{}.partial",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    path.into()
}

pub fn execute(mut job: Job, caller: &mut dyn Callbacks) -> Outcome {
    let mut callbacks = Timed::new(caller, job.timeout_millis);
    let mut temporary = Temporary(None);
    let result = (|| -> Result<i32, Failure> {
        callbacks.check()?;
        job.request.validate()?;
        let input = std::path::absolute(&job.request.input)?;
        job.request.input = input.to_string_lossy().into_owned();
        let output = job
            .request
            .output
            .as_deref()
            .map(std::path::absolute)
            .transpose()?;
        if let Some(output) = &output {
            if files::ordinal_ignore_case(&input.to_string_lossy(), &output.to_string_lossy()) {
                return Err(Failure::new("Io", "音频输入输出不能是同一文件。"));
            }
            if output.is_file() && !job.replace {
                return Err(Failure::new(
                    "Io",
                    &format!("输出文件已存在：{}", output.display()),
                ));
            }
            let path = temporary_path(output);
            job.request.output = Some(path.to_string_lossy().into_owned());
            temporary.0 = Some(path);
        }
        let media = Media::load(&job.library)?;
        let status = media.run(&job.request, &mut callbacks)?;
        callbacks.check()?;
        if status >= 0
            && let (Some(path), Some(output)) = (&temporary.0, &output)
        {
            files::move_file(path, output, job.replace)?;
            temporary.0 = None;
        }
        Ok(i32::from(status < 0))
    })();
    // Like the original finally block, a cleanup failure is observable to the caller.
    let result = temporary.clean().map_err(Failure::from).and(result);
    match result {
        Ok(code) => Outcome {
            exit_code: Some(code),
            failure: None,
        },
        Err(error) => Outcome {
            exit_code: None,
            failure: Some(error),
        },
    }
}
