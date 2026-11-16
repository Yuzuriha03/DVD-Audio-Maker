//! Task-owned files, isolated from input folders and previously published images.
use crate::platform::OpenOptionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Debug)]
pub struct Workspace {
    path: PathBuf,
    _lease: fs::File,
    retain_unfinished: bool,
    finished: AtomicBool,
}

impl Workspace {
    #[must_use]
    pub fn unfinished(build: &Path) -> bool {
        let path = build.join("unfinished-task");
        path.join(".owner").is_file() && !path.join(".finished").exists()
    }

    /// # Errors
    /// Returns an error for an unresolved unfinished task, a reparse-point
    /// ancestor, an unowned existing task directory, or a filesystem failure
    /// while locking, creating, or resetting the task and its ownership marker.
    pub fn create_task(build: &Path, retain: bool, restart: Option<bool>) -> Result<Self, String> {
        create_directories(build)?;
        reject_reparse(build)?;
        let path = build.join("unfinished-task");
        if Self::unfinished(build) && restart.is_none() {
            return Err(
                "An unfinished task exists. Choose Continue production or Restart production."
                    .into(),
            );
        }
        let task = Self::open(path, retain)?;
        if restart == Some(true) {
            fs::remove_dir_all(&task.path).map_err(|e| e.to_string())?;
            create_directories(&task.path)?;
            fs::write(task.path.join(".owner"), b"dvda-task-v1").map_err(|e| e.to_string())?;
        }
        Ok(task)
    }

    /// # Errors
    /// Returns an error if the workspace root cannot be inspected or created,
    /// an ancestor is a reparse point, a resumed directory is unowned, or the
    /// workspace cannot be locked, initialized, or reset after completion.
    pub fn create(build: &Path, resume: bool) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let root = if resume {
            build.join("unfinished-task")
        } else {
            #[cfg(target_os = "android")]
            {
                scratch_root(build)?
            }
            #[cfg(not(target_os = "android"))]
            {
                scratch_root(build)
            }
        };
        if resume {
            create_directories(build)?;
            reject_reparse(build)?;
            return Self::open(root, true);
        }
        create_directories(&root)?;
        reject_reparse(&root)?;
        for entry in fs::read_dir(&root)
            .map_err(|error| {
                format!(
                    "Cannot enumerate task workspace {}: {error}",
                    root.display()
                )
            })?
            .flatten()
        {
            let path = entry.path();
            if !entry.file_name().to_string_lossy().starts_with("task-")
                || !entry.file_type().is_ok_and(|t| t.is_dir())
                || fs::symlink_metadata(&path).is_ok_and(|m| {
                    use crate::platform::MetadataExt;
                    m.file_attributes() & 0x400 != 0
                })
                || fs::read(path.join(".owner")).ok().as_deref() != Some(b"dvda-task-v1")
            {
                continue;
            }
            if let Ok(_file) = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .custom_flags(0x0400_0000)
                .open(path.with_extension("lease"))
            {
                let _ = fs::remove_dir_all(path);
            }
        }
        loop {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = root.join(format!(
                "task-{}-{stamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self::open(path, false),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(format!(
                        "Cannot create task workspace {}: {error}",
                        path.display()
                    ));
                }
            }
        }
    }
    fn open(path: PathBuf, retain_unfinished: bool) -> Result<Self, String> {
        let lease_path = if path
            .file_name()
            .is_some_and(|name| name == "unfinished-task")
        {
            path.with_file_name(".unfinished-task.lease")
        } else {
            path.with_extension("lease")
        };
        let lease = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .custom_flags(0x0400_0000)
            .open(lease_path)
            .map_err(|e| format!("Cannot lock task workspace {}: {e}", path.display()))?;
        if retain_unfinished && path.exists() {
            reject_reparse(&path)?;
            // A `.finished` directory is a completed task whose deletion failed
            // (possibly after removing `.owner`); it is safe to remove, while a
            // live unfinished task must still prove ownership.
            if !path.join(".finished").exists()
                && fs::read(path.join(".owner")).ok().as_deref() != Some(b"dvda-task-v1")
            {
                return Err(format!(
                    "Refusing to adopt an unowned task directory: {}",
                    path.display()
                ));
            }
        }
        if retain_unfinished && path.join(".finished").exists() {
            remove_task_directory(&path, true)?;
        }
        create_directories(&path)?;
        fs::write(path.join(".owner"), b"dvda-task-v1").map_err(|e| e.to_string())?;
        Ok(Self {
            path,
            _lease: lease,
            retain_unfinished,
            finished: AtomicBool::new(false),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn discard(&self) {
        // Retain remains the safe default until the user explicitly chooses deletion.
        self.finish();
    }
    pub fn finish(&self) {
        self.finished.store(true, Ordering::Relaxed);
    }

    /// Best-effort completed marker so a partially failed delete is recognized
    /// as our finished task on retry, even when `.owner` was already removed.
    fn write_finished_marker(&self) {
        if self.retain_unfinished {
            mark_finished(&self.path);
        }
    }

    /// Marks the task finished and removes its directory, reporting failures.
    ///
    /// This is the same lifecycle Drop uses; callers that must distinguish
    /// "really removed" from "left behind" use this instead of relying on the
    /// best-effort Drop cleanup.
    ///
    /// # Errors
    /// Returns an error when the task directory exists and cannot be removed.
    pub fn finish_and_remove(&self) -> Result<(), String> {
        self.finish();
        self.write_finished_marker();
        remove_task_directory(&self.path, self.retain_unfinished)
    }

    /// Removes an unfinished, finished, or partially deleted task directory.
    ///
    /// Returns `Ok(false)` only when nothing exists to delete. Errors leave the
    /// completed marker in place so the caller can retry the same lifecycle.
    ///
    /// # Errors
    /// Returns an error for an unowned existing directory or a failed deletion.
    pub fn discard_task_directory(build: &Path) -> Result<bool, String> {
        if build.as_os_str().is_empty() {
            return Ok(false);
        }
        let path = build.join("unfinished-task");
        if !path.exists() {
            let _ = fs::remove_file(build.join(".unfinished-task.lease"));
            return Ok(false);
        }
        reject_reparse(build)?;
        // This path is the app-reserved task directory and discard is an explicit
        // user action: old/partially deleted leftovers without `.owner` must be
        // removable too, while adoption by new tasks stays ownership-checked.
        reject_reparse(&path)?;
        remove_task_directory(&path, true)?;
        let _ = fs::remove_file(build.join(".unfinished-task.lease"));
        Ok(true)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if self.retain_unfinished && self.finished.load(Ordering::Relaxed) {
            let _ = fs::write(self.path.join(".finished"), b"completed");
        }
        if (!self.retain_unfinished || self.finished.load(Ordering::Relaxed))
            && let Err(error) = remove_task_directory(&self.path, self.retain_unfinished)
        {
            eprintln!(
                "[cleanup] {}: {error}; retry on next startup",
                self.path.display()
            );
        }
    }
}

/// Writes the completed marker on a retained task directory.
fn mark_finished(path: &Path) {
    let _ = fs::create_dir_all(path);
    let _ = fs::write(path.join(".finished"), b"completed");
}

/// Single safe task-directory deletion used by finish, Drop, open-retry and discard.
///
/// A failed delete can remove `.owner`/`.finished` before hitting a locked file;
/// restoring the marker here keeps the leftover recognizable and retryable.
fn remove_task_directory(path: &Path, restore_marker: bool) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            if restore_marker {
                mark_finished(path);
            }
            Err(format!(
                "Cannot remove task workspace {}: {error}",
                path.display()
            ))
        }
    }
}
fn create_directories(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "Cannot create task workspace directory {}: {error}",
            path.display()
        )
    })
}

#[cfg(target_os = "android")]
fn scratch_root(build: &Path) -> Result<PathBuf, String> {
    if build.as_os_str().is_empty() {
        return Err("Android task workspace requires an app-private build directory".into());
    }
    Ok(build.join("scratch-tasks"))
}

#[cfg(not(target_os = "android"))]
fn scratch_root(_build: &Path) -> PathBuf {
    std::env::temp_dir().join("DVD-Audio-Maker-tasks")
}

#[cfg(target_os = "android")]
fn reject_reparse(path: &Path) -> Result<(), String> {
    use crate::platform::MetadataExt;
    // Do not stat /data, /data/user or other platform-owned ancestors. Android
    // deliberately denies those probes on some builds, and /data/user/<id> is
    // itself a system alias. The Flutter shell supplies only app-private roots
    // and its SAF mirror contains regular files/directories, never symlinks.
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Cannot inspect task workspace {}: {error}", path.display()))?;
    if metadata.file_attributes() & 0x400 != 0 {
        return Err(format!(
            "Task workspace cannot be a reparse point: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn reject_reparse(path: &Path) -> Result<(), String> {
    use crate::platform::MetadataExt;
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor)
            .map_err(|error| {
                format!(
                    "Cannot inspect task workspace {}: {error}",
                    ancestor.display()
                )
            })?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err(format!(
                "Task workspace cannot traverse a reparse point: {}",
                ancestor.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_never_adopts_unowned_directory() {
        let container = Workspace::create(Path::new("unused"), false).unwrap();
        let build = container.path().join("build");
        let task = build.join("unfinished-task");
        fs::create_dir_all(&task).unwrap();
        fs::write(task.join("user-file"), b"preserve").unwrap();
        assert!(Workspace::create_task(&build, true, Some(true)).is_err());
        assert_eq!(fs::read(task.join("user-file")).unwrap(), b"preserve");
    }

    #[test]
    fn explicit_discard_deletes_retained_task() {
        let container = Workspace::create(Path::new("unused"), false).unwrap();
        let task = Workspace::create_task(&container.path().join("build"), true, None).unwrap();
        let path = task.path().to_owned();
        task.discard();
        drop(task);
        assert!(!path.exists());
    }

    #[test]
    fn abandoned_task_requires_explicit_continue_or_restart() {
        let container = Workspace::create(Path::new("unused"), false).unwrap();
        let build = container.path().join("build");
        let task = Workspace::create_task(&build, false, None).unwrap();
        let path = task.path().to_owned();
        fs::write(path.join("progress"), b"saved").unwrap();
        // Simulate an abnormal exit leaving task files but releasing the lease.
        let mut task = task;
        task.retain_unfinished = true;
        drop(task);
        assert!(Workspace::unfinished(&build));
        assert!(Workspace::create_task(&build, false, None).is_err());
        let task = Workspace::create_task(&build, false, Some(false)).unwrap();
        assert_eq!(fs::read(task.path().join("progress")).unwrap(), b"saved");
        // The lease is only exclusive on Windows; elsewhere the sharing shim is a
        // no-op, so a concurrent create_task is allowed and cannot be asserted here.
        #[cfg(windows)]
        assert!(Workspace::create_task(&build, false, Some(true)).is_err());
        drop(task);
        assert!(!path.exists());
        let task = Workspace::create_task(&build, true, None).unwrap();
        fs::write(task.path().join("progress"), b"old").unwrap();
        drop(task);
        let task = Workspace::create_task(&build, true, Some(true)).unwrap();
        assert!(!task.path().join("progress").exists());
        task.finish();
        drop(task);
        assert!(!path.exists());
    }

    /// Keeping live task directories out of a later cleanup relies on the Windows
    /// lease being exclusive. Non-Windows platforms implement the lease as an
    /// advisory no-op, so the cleanup path would remove a live task and this
    /// scenario is only meaningful on Windows.
    #[cfg(windows)]
    #[test]
    fn cleanup_preserves_live_tasks_and_only_retains_unfinished_resume() {
        let first = Workspace::create(Path::new("unused"), false).unwrap();
        let path = first.path().to_owned();
        fs::write(path.join("encoded.mlp"), b"temporary").unwrap();
        let second = Workspace::create(Path::new("unused"), false).unwrap();
        assert!(path.join("encoded.mlp").is_file());
        drop(first);
        assert!(!path.exists());
        let build = second.path().join("build");
        let task = Workspace::create(&build, true).unwrap();
        let retained = task.path().to_owned();
        assert!(Workspace::create(&build, true).is_err());
        drop(task);
        assert!(retained.exists());
        let task = Workspace::create(&build, true).unwrap();
        task.finish();
        drop(task);
        assert!(!retained.exists());
    }
}
