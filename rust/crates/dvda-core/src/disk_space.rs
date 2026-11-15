//! Advisory free-space checks, shared by every platform.
//!
//! One implementation estimates the required bytes and formats the advisories;
//! only the two operating-system queries below differ per platform.
use crate::{
    build::Job,
    disc::{Disc, Track},
    workflow,
};
use dvda_native::media::Callbacks;
use serde_json::{Value, json};
use std::path::Path;

#[cfg(windows)]
mod platform {
    //! Win32 free-space query.
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Path, PathBuf};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }

    /// Volume root (for example `C:\\`) that owns `path`.
    pub(super) fn volume_root(path: &Path) -> String {
        let Ok(path) = std::path::absolute(path) else {
            return "?".into();
        };
        let mut root = PathBuf::new();
        for part in path.components() {
            if matches!(part, Component::Prefix(_) | Component::RootDir) {
                root.push(part.as_os_str());
            } else {
                break;
            }
        }
        root.to_string_lossy().into_owned()
    }

    /// Free bytes on the volume named by `root`, or None when unreadable.
    pub(super) fn available(root: &str) -> Option<u64> {
        let wide: Vec<_> = std::ffi::OsStr::new(root)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut bytes = 0;
        // SAFETY: NUL-terminated path and a live output pointer; unused outputs are null.
        (unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &raw mut bytes,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0)
            .then_some(bytes)
    }
}

#[cfg(not(windows))]
mod platform {
    //! POSIX `statvfs` free-space query.
    use std::path::{Component, Path, PathBuf};

    /// Volume root (for example `/data`) that owns `path`.
    pub(super) fn volume_root(path: &Path) -> String {
        let Ok(path) = std::path::absolute(path) else {
            return "?".into();
        };
        let mut parts = path.components();
        let mut root = PathBuf::new();
        if matches!(parts.next(), Some(Component::RootDir)) {
            root.push("/");
        }
        if let Some(first) = parts.next() {
            root.push(first.as_os_str());
        }
        root.to_string_lossy().into_owned()
    }

    /// Free bytes on the filesystem named by `root`, or None when unreadable.
    pub(super) fn available(root: &str) -> Option<u64> {
        let path = std::ffi::CString::new(root).ok()?;
        // SAFETY: statvfs only reads the NUL-terminated path and fills the
        // provided structure when it succeeds.
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &raw mut stat) } != 0 {
            return None;
        }
        let free = u64::try_from(stat.f_bavail).unwrap_or(0);
        let unit = u64::try_from(stat.f_frsize).unwrap_or(0);
        Some(free.saturating_mul(unit))
    }
}

/// Returns advisory low-space diagnostics for the job's estimated volume usage.
/// An unavailable free-space reading is logged as unknown, not treated as an error.
///
/// # Errors
/// Returns an error if requirement estimation fails, or its response is not an
/// array of entries with a string volume root and an unsigned byte requirement.
pub fn check(
    job: &Job,
    tracks: &[Track],
    discs: Option<&[Disc]>,
    caller: &mut dyn Callbacks,
) -> Result<Vec<Value>, String> {
    let groups = workflow::dispatch(
        "disk.estimate_requirements",
        json!({
            "MlpSource":job.mlp_source, "MlpRoot":platform::volume_root(Path::new(&job.mlp_external_directory)),
            "BuildRoot":platform::volume_root(&job.build_directory), "OutputRoot":platform::volume_root(&job.output_root),
            "FinalRoot":platform::volume_root(&job.final_directory), "KeepIntermediate":job.keep_intermediate,
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
        if let Some(available) = platform::available(root) {
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
