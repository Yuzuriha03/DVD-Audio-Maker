//! Image job transactions over the existing C component's in-process ABI.
use crate::media::{Failure, Outcome, Temporary, Timed, temporary_path};
use dvda_native::{files, media::Callbacks};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Output {
    pub path: PathBuf,
    pub argument_index: usize,
    pub format_prefix: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub library: PathBuf,
    pub arguments: Vec<String>,
    pub output: Option<Output>,
    pub timeout_millis: Option<u64>,
}

pub fn execute(mut job: Job, caller: &mut dyn Callbacks) -> Outcome {
    let mut callbacks = Timed::new(caller, job.timeout_millis);
    let mut temporary = Temporary(None);
    let result = (|| -> Result<i32, Failure> {
        callbacks.check()?;
        if let Some(output) = &mut job.output {
            if output.argument_index >= job.arguments.len() {
                return Err(Failure::new("Argument", "Invalid image output argument"));
            }
            output.path = std::path::absolute(&output.path)?;
            // Retain the real suffix: the C writer selects its codec from the filename.
            let path = temporary_path(&output.path)
                .with_extension(output.path.extension().unwrap_or_default());
            job.arguments[output.argument_index] =
                format!("{}{}", output.format_prefix, path.display());
            temporary.0 = Some(path);
        }
        let images = crate::native_components::images(Some(&job.library))?;
        let status = images.run(&job.arguments, &mut callbacks)?;
        callbacks.check()?;
        if status == 0
            && let (Some(path), Some(output)) = (&temporary.0, &job.output)
        {
            files::move_file(path, &output.path, true)?;
            temporary.0 = None;
        }
        Ok(status)
    })();
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
