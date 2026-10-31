//! Task-owned files, isolated from input folders and previously published images.
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
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
    pub fn unfinished(build: &Path) -> bool {
        let path = build.join("unfinished-task");
        path.join(".owner").is_file() && !path.join(".finished").exists()
    }

    pub fn create_task(build: &Path, retain: bool, restart: Option<bool>) -> Result<Self, String> {
        fs::create_dir_all(build).map_err(|e| e.to_string())?;
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
            fs::create_dir_all(&task.path).map_err(|e| e.to_string())?;
            fs::write(task.path.join(".owner"), b"dvda-task-v1").map_err(|e| e.to_string())?;
        }
        Ok(task)
    }

    pub fn create(build: &Path, resume: bool) -> Result<Self, String> {
        let root = if resume {
            build.join("unfinished-task")
        } else {
            std::env::temp_dir().join("DVD-Audio-Maker-tasks")
        };
        if resume {
            fs::create_dir_all(build).map_err(|e| e.to_string())?;
            reject_reparse(build)?;
            return Self::open(root, true);
        }
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        reject_reparse(&root)?;
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
            let path = entry.path();
            if !entry.file_name().to_string_lossy().starts_with("task-")
                || !entry.file_type().is_ok_and(|t| t.is_dir())
                || fs::symlink_metadata(&path).is_ok_and(|m| {
                    use std::os::windows::fs::MetadataExt;
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
                .custom_flags(0x04000000)
                .open(path.with_extension("lease"))
            {
                let _ = fs::remove_dir_all(path);
            }
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
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
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
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
            .custom_flags(0x04000000)
            .open(lease_path)
            .map_err(|e| format!("Cannot lock task workspace {}: {e}", path.display()))?;
        if retain_unfinished && path.exists() {
            reject_reparse(&path)?;
            if fs::read(path.join(".owner")).ok().as_deref() != Some(b"dvda-task-v1") {
                return Err(format!(
                    "Refusing to adopt an unowned task directory: {}",
                    path.display()
                ));
            }
        }
        if retain_unfinished && path.join(".finished").exists() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
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
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if self.retain_unfinished && self.finished.load(Ordering::Relaxed) {
            let _ = fs::write(self.path.join(".finished"), b"completed");
        }
        if (!self.retain_unfinished || self.finished.load(Ordering::Relaxed))
            && let Err(error) = fs::remove_dir_all(&self.path)
        {
            eprintln!(
                "[cleanup] {}: {error}; retry on next startup",
                self.path.display()
            );
        }
    }
}

fn reject_reparse(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor)
            .map_err(|e| e.to_string())?
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
