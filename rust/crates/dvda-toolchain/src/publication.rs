//! Candidates live next to final outputs; no final file is removed before success.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct Candidate {
    pub root: PathBuf,
}
impl Candidate {
    pub fn new(output: &Path) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(output).map_err(err)?;
        loop {
            let root = output.join(format!(
                ".package-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => return Ok(Self { root }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(err(e)),
            }
        }
    }
}
impl Drop for Candidate {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Recovery {
    root: PathBuf,
    retain: bool,
}
impl Recovery {
    fn new(output: &Path) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(output).map_err(err)?;
        loop {
            let root = output.join(format!(
                "release-recovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    return Ok(Self {
                        root,
                        retain: false,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(err(e)),
            }
        }
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

pub fn commit(candidate: &Path, output: &Path, relative_files: &[PathBuf]) -> Result<(), String> {
    commit_with(candidate, output, relative_files, replace)
}

fn commit_with(
    candidate: &Path,
    output: &Path,
    relative_files: &[PathBuf],
    mut mover: impl FnMut(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    // Backups are already in their final recovery location before publication.
    // A failed rollback must not depend on another move succeeding, nor on the
    // lifetime of the disposable build candidate.
    let mut recovery = Recovery::new(output)?;
    let mut changes: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    // Back up the whole set before replacing any published file. A permission
    // error during backup therefore cannot partially publish the candidate.
    for relative in relative_files {
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Invalid relative release path".into());
        }
        let source = candidate.join(relative);
        if !source.is_file() {
            return Err(format!("Missing release candidate: {}", source.display()));
        }
        let destination = output.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(err)?;
        }
        let backup = if destination.exists() {
            if !destination.is_file() {
                return Err(format!(
                    "Release destination is not a file: {}",
                    destination.display()
                ));
            }
            let backup = recovery.root.join(relative);
            fs::create_dir_all(backup.parent().unwrap()).map_err(err)?;
            fs::copy(&destination, &backup).map_err(err)?;
            Some(backup)
        } else {
            None
        };
        changes.push((destination, backup));
    }
    for (index, relative) in relative_files.iter().enumerate() {
        if let Err(error) = mover(&candidate.join(relative), &changes[index].0) {
            let mut failures = Vec::new();
            for (destination, backup) in changes[..index].iter().rev() {
                let result = if let Some(backup) = backup {
                    replace(backup, destination)
                } else {
                    fs::remove_file(destination).map_err(err)
                };
                if let Err(e) = result {
                    failures.push(format!("{}: {e}", destination.display()));
                }
            }
            if failures.is_empty() {
                return Err(format!(
                    "Publication failed; previous release preserved: {error}"
                ));
            }
            recovery.retain = true;
            return Err(format!(
                "Publication failed: {error}; rollback errors: {}; backups preserved at {}",
                failures.join("; "),
                recovery.root.display()
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 1 | 8) } == 0 {
        Err(err(std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(err)
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::file_hash;
    #[test]
    fn failed_build_copy_and_commit_preserve_published_exe_and_zip() {
        let f = crate::validation::tests::Fixture::new();
        let output = f.root.join("output");
        fs::create_dir(&output).unwrap();
        let names = [
            PathBuf::from("DVD-Audio-Maker/DVD-Audio-Maker.exe"),
            PathBuf::from("DVD-Audio-Maker-v1.0-win-x64.zip"),
        ];
        fs::create_dir(output.join("DVD-Audio-Maker")).unwrap();
        for name in &names {
            fs::write(output.join(name), b"previous release").unwrap();
        }
        let expected: Vec<_> = names
            .iter()
            .map(|n| file_hash(&output.join(n)).unwrap())
            .collect();
        {
            let candidate = Candidate::new(&output).unwrap();
            fs::write(candidate.root.join("partial.exe"), b"failed build").unwrap();
        }
        let candidate = Candidate::new(&output).unwrap();
        assert!(commit(&candidate.root, &output, &names).is_err()); // failed candidate copy
        fs::create_dir(candidate.root.join("DVD-Audio-Maker")).unwrap();
        for name in &names {
            fs::write(candidate.root.join(name), b"new release").unwrap();
        }
        let mut calls = 0;
        let error = commit_with(&candidate.root, &output, &names, |source, destination| {
            calls += 1;
            if calls == 2 {
                Err("injected publication failure".into())
            } else {
                replace(source, destination)
            }
        })
        .unwrap_err();
        assert!(error.contains("previous release preserved"));
        for (index, name) in names.iter().enumerate() {
            assert_eq!(file_hash(&output.join(name)).unwrap(), expected[index]);
        }
        for name in &names {
            fs::write(candidate.root.join(name), b"new release").unwrap();
        }
        commit(&candidate.root, &output, &names).unwrap();
        for name in &names {
            assert_eq!(fs::read(output.join(name)).unwrap(), b"new release");
        }
    }

    #[cfg(windows)]
    #[test]
    fn real_locked_zip_rejects_publication_and_restores_the_exe() {
        use std::os::windows::fs::OpenOptionsExt;
        let f = crate::validation::tests::Fixture::new();
        let output = f.root.join("locked-release");
        fs::create_dir(&output).unwrap();
        let names = [PathBuf::from("app.exe"), PathBuf::from("release.zip")];
        for name in &names {
            fs::write(output.join(name), b"old").unwrap();
        }
        let candidate = Candidate::new(&output).unwrap();
        for name in &names {
            fs::write(candidate.root.join(name), b"new").unwrap();
        }
        let mut lock = None;
        let error = commit_with(&candidate.root, &output, &names, |source, destination| {
            if lock.is_none() {
                // Lock after backup has succeeded, so EXE replacement occurs
                // before the real Windows ZIP sharing violation.
                lock = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(0)
                        .open(output.join("release.zip"))
                        .unwrap(),
                );
            }
            replace(source, destination)
        })
        .unwrap_err();
        assert!(error.contains("previous release preserved"));
        drop(lock);
        for name in &names {
            assert_eq!(fs::read(output.join(name)).unwrap(), b"old");
        }
        assert!(fs::read_dir(&output).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("release-recovery-")
        }));
    }

    #[cfg(windows)]
    #[test]
    fn failed_rollback_retains_backups_after_candidate_drop_and_existing_recovery() {
        use std::os::windows::fs::OpenOptionsExt;
        let f = crate::validation::tests::Fixture::new();
        let output = f.root.join("double-locked-release");
        fs::create_dir(&output).unwrap();
        // Reproduce an existing recovery destination from an earlier failure.
        let prior_recovery = output.join(format!("release-recovery-{}", std::process::id()));
        fs::create_dir(&prior_recovery).unwrap();
        fs::write(prior_recovery.join("app.exe"), b"previous recovery").unwrap();
        let names = [PathBuf::from("app.exe"), PathBuf::from("release.zip")];
        for name in &names {
            fs::write(output.join(name), b"old").unwrap();
        }
        let candidate = Candidate::new(&output).unwrap();
        let candidate_path = candidate.root.clone();
        for name in &names {
            fs::write(candidate.root.join(name), b"new").unwrap();
        }
        let mut locks = Vec::new();
        let error = commit_with(&candidate.root, &output, &names, |source, destination| {
            if locks.is_empty() {
                locks.push(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(output.join("release.zip"))
                        .unwrap(),
                );
                replace(source, destination)?;
                // A reader opens the newly published EXE before ZIP publication.
                // This real lock blocks restoration of the old EXE as well.
                locks.push(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(destination)
                        .unwrap(),
                );
                Ok(())
            } else {
                replace(source, destination)
            }
        })
        .unwrap_err();
        assert!(error.contains("rollback errors"));
        drop(candidate);
        assert!(!candidate_path.exists());
        let recovery = fs::read_dir(&output)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir() && p != &prior_recovery)
            .unwrap();
        assert!(error.contains(recovery.to_str().unwrap()));
        assert_eq!(fs::read(output.join("app.exe")).unwrap(), b"new");
        assert_eq!(fs::read(output.join("release.zip")).unwrap(), b"old");
        for name in &names {
            assert_eq!(fs::read(recovery.join(name)).unwrap(), b"old");
        }
        assert_eq!(
            fs::read(prior_recovery.join("app.exe")).unwrap(),
            b"previous recovery"
        );
        drop(locks);
        for name in &names {
            replace(&recovery.join(name), &output.join(name)).unwrap();
            assert_eq!(fs::read(output.join(name)).unwrap(), b"old");
        }
    }
}
