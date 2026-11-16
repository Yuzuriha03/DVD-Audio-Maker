//! Small platform abstraction over Win32 file-detail APIs.
#[cfg(windows)]
pub use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

#[cfg(not(windows))]
pub trait OpenOptionsExt {
    fn share_mode(&mut self, value: u32) -> &mut Self;
    fn custom_flags(&mut self, value: i32) -> &mut Self;
}

#[cfg(not(windows))]
impl OpenOptionsExt for std::fs::OpenOptions {
    fn share_mode(&mut self, _value: u32) -> &mut Self {
        self
    }
    fn custom_flags(&mut self, _value: i32) -> &mut Self {
        self
    }
}

#[cfg(not(windows))]
pub trait MetadataExt {
    fn file_size(&self) -> u64;
    fn last_write_time(&self) -> u64;
    fn file_attributes(&self) -> u32;
}

#[cfg(not(windows))]
impl MetadataExt for std::fs::Metadata {
    fn file_size(&self) -> u64 {
        self.len()
    }

    fn last_write_time(&self) -> u64 {
        self.modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| {
                u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
            })
    }

    fn file_attributes(&self) -> u32 {
        // FILE_ATTRIBUTE_REPARSE_POINT equivalent: symlinks are the only
        // reparse-like entries on this platform.
        if self.file_type().is_symlink() {
            0x400
        } else {
            0
        }
    }
}

/// External author/runtime library directory that the shared canonical disc
/// signature must hash.
///
/// Windows packages keep author/media libraries next to the executable. Android
/// links those components statically, so there is no readable external library
/// directory there; returning `None` lets the one shared canonical
/// implementation keep strict permission errors for directories that *are*
/// provided (it never suppresses `EACCES` for a real directory).
#[must_use]
pub fn author_library_directory(executable: &std::path::Path) -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        executable.parent().map(std::path::Path::to_path_buf)
    }
    #[cfg(not(windows))]
    {
        let _ = executable;
        None
    }
}

#[cfg(test)]
mod provider_tests {
    use super::author_library_directory;
    use std::path::Path;

    #[test]
    fn external_author_libraries_are_platform_specific() {
        let executable = Path::new("/some/app/bin/tool");
        let resolved = author_library_directory(executable);
        #[cfg(windows)]
        assert_eq!(resolved.as_deref(), executable.parent());
        #[cfg(not(windows))]
        assert!(
            resolved.is_none(),
            "statically linked layout has no library directory"
        );
    }
}
