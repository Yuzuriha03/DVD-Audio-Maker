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
