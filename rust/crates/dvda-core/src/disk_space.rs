//! Advisory space checks, using the same estimates as the former desktop.
use crate::{
    build::Job,
    disc::{Disc, Track},
    workflow,
};
use dvda_native::media::Callbacks;
use serde_json::{Value, json};
use std::{
    os::windows::ffi::OsStrExt,
    path::{Component, Path},
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetDiskFreeSpaceExW(
        directory: *const u16,
        available: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> i32;
}
fn root(path: &Path) -> String {
    let Ok(path) = std::path::absolute(path) else {
        return "?".into();
    };
    let mut root = std::path::PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::Prefix(_) | Component::RootDir) {
            root.push(part.as_os_str());
        } else {
            break;
        }
    }
    root.to_string_lossy().into_owned()
}
fn available(root: &str) -> Option<u64> {
    let wide: Vec<_> = std::ffi::OsStr::new(root)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut bytes = 0;
    // SAFETY: NUL-terminated path and a live output pointer; unused outputs are null.
    (unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut bytes,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0)
        .then_some(bytes)
}
pub fn check(
    job: &Job,
    tracks: &[Track],
    discs: Option<&[Disc]>,
    caller: &mut dyn Callbacks,
) -> Result<Vec<Value>, String> {
    let groups = workflow::dispatch(
        "disk.estimate_requirements",
        json!({
            "MlpSource":job.mlp_source, "MlpRoot":root(Path::new(&job.mlp_external_directory)),
            "BuildRoot":root(&job.build_directory), "OutputRoot":root(&job.output_root),
            "FinalRoot":root(&job.final_directory), "KeepIntermediate":job.keep_intermediate,
            "MenuEnabled":job.menu_enabled, "SampleRate":job.mlp_sample_rate, "Bits":job.mlp_bits,
            "Tracks":tracks,"Discs":discs
        }),
    )?;
    let mut diagnostics = Vec::new();
    for group in groups.as_array().ok_or("Invalid disk estimate")? {
        let root = group["Root"].as_str().ok_or("Missing volume root")?;
        let purpose = group["Purpose"].as_str().unwrap_or_default();
        let required = group["RequiredBytes"]
            .as_u64()
            .ok_or("Invalid disk estimate")?;
        if let Some(available) = available(root) {
            caller.emit(
                1,
                &format!(
                    "[space] {root}: {purpose}, required {required} B, available {available} B"
                ),
            );
            if available < required {
                let message = format!(
                    "{root} 上“{purpose}”预计需要 {required} 字节，但仅剩 {available} 字节。估算为保守值，仅供参考。"
                );
                caller.emit(2, &message);
                diagnostics.push(json!({"Severity":1,"Code":"DISK_SPACE_LOW","Message":message}));
            }
        } else {
            caller.emit(
                1,
                &format!("[space] {root}: {purpose}, required {required} B, available unknown"),
            );
        }
    }
    Ok(diagnostics)
}
