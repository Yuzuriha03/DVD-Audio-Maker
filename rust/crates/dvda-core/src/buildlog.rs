//! Build log header formatting.
use dvda_native::media::Callbacks;
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path};

pub struct Writer<'a> {
    file: fs::File,
    caller: &'a mut dyn Callbacks,
    failed: bool,
}
impl<'a> Writer<'a> {
    pub fn open(path: &Path, caller: &'a mut dyn Callbacks) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            file,
            caller,
            failed: false,
        })
    }
}
impl Callbacks for Writer<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        if !self.failed
            && let Err(error) = writeln!(self.file, "{text}").and_then(|_| self.file.flush())
        {
            self.failed = true;
            self.caller
                .emit(2, &format!("Cannot write build log: {error}"));
        }
        self.caller.emit(stream, text);
    }
    fn cancelled(&mut self) -> bool {
        self.failed || self.caller.cancelled()
    }
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    if operation != "build.log_header" {
        return Err(format!("Unsupported build log operation: {operation}"));
    }
    let object = request.as_object().ok_or("Expected build log request")?;
    let dry_run = object
        .get("DryRun")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let generated = text(object, "Generated");
    let author = text(object, "DvdaAuthor");
    let output = text(object, "FinalDirectory");
    let iso_prefix = text(object, "IsoPrefix");
    let title = text(object, "Title");
    let mut lines = vec![
        String::new(),
        "=".repeat(60),
        format!(
            "[Rust build]{} {generated}",
            if dry_run { " [DRY-RUN]" } else { "" }
        ),
        format!("  dvda-author : {author}"),
        "  ISO writer  : dvda-author in-process C writer".into(),
        format!("  output      : {output}"),
        format!("  iso prefix  : {iso_prefix}"),
        format!("  title       : {title}"),
    ];
    if dry_run {
        lines.push("  注: dry-run 未执行 dvda-author，本文件不含轨道表；".into());
        lines.push("      审计请用 build.log（上次真出盘）。".into());
    }
    lines.push("=".repeat(60));
    Ok(json!(lines))
}

fn text(object: &serde_json::Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_formal_and_dry_run_headers() {
        let request = json!({
            "DryRun":true,"Generated":"2026-10-05 12:34:56",
            "DvdaAuthor":"author.exe","FinalDirectory":"out",
            "IsoPrefix":"disc","Title":"Title"
        });
        let lines = dispatch("build.log_header", request).unwrap();
        assert_eq!(lines[0], "");
        assert_eq!(lines[2], "[Rust build] [DRY-RUN] 2026-10-05 12:34:56");
        assert!(lines[8].as_str().unwrap().contains("dry-run"));
        assert!(lines[9].as_str().unwrap().contains("build.log"));
        assert_eq!(lines[10], "=".repeat(60));
    }
}
