//! Windows file operations used by publication. Moves never replace unless requested.
use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt, path::Path};
mod ordinal_case;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    fn CopyFileW(source: *const u16, destination: *const u16, fail_if_exists: i32) -> i32;
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut result: Vec<_> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(io::Error::from_raw_os_error(123));
    }
    result.push(0);
    Ok(result)
}

pub fn move_file(source: &Path, destination: &Path, replace: bool) -> io::Result<()> {
    let source = wide(source.as_os_str())?;
    let destination = wide(destination.as_os_str())?;
    // COPY_ALLOWED retains File.Move's cross-volume behavior; no replacement by default.
    let flags = 2 | u32::from(replace);
    // SAFETY: both NUL-terminated buffers remain valid for the synchronous call.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn copy_file(source: &Path, destination: &Path, replace: bool) -> io::Result<()> {
    let source = wide(source.as_os_str())?;
    let destination = wide(destination.as_os_str())?;
    // SAFETY: both NUL-terminated buffers remain valid for the synchronous call.
    if unsafe { CopyFileW(source.as_ptr(), destination.as_ptr(), i32::from(!replace)) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn ordinal_ignore_case(left: &str, right: &str) -> bool {
    ordinal_ignore_case_cmp(left, right).is_eq()
}

/// Match the frozen .NET ordinal baseline, including supplementary letters and
/// BMP symbol variants, without linguistic expansions or Unicode normalization.
pub fn ordinal_ignore_case_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    // .NET's ordinal ignore-case comparer orders valid supplementary scalars
    // after BMP characters. Raw UTF-16 unit order would incorrectly sort them
    // before U+E000..U+FFFF.
    left.chars()
        .map(ordinal_case::uppercase)
        .cmp(right.chars().map(ordinal_case::uppercase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinal_supplementary_case_matches_dotnet_without_expanding_sharp_s() {
        // Frozen against .NET 10 StringComparer.OrdinalIgnoreCase. Each row
        // covers a separate supplementary-plane alphabet with casing.
        for (upper, lower) in [
            ('\u{10400}', '\u{10428}'), // Deseret
            ('\u{104b0}', '\u{104d8}'), // Osage
            ('\u{10c80}', '\u{10cc0}'), // Old Hungarian
            ('\u{118a0}', '\u{118c0}'), // Warang Citi
            ('\u{16e40}', '\u{16e60}'), // Medefaidrin
            ('\u{1e900}', '\u{1e922}'), // Adlam
        ] {
            let upper = format!("C:\\Music\\{upper}É.mlp");
            let lower = format!("c:\\music\\{lower}é.MLP");
            assert!(ordinal_ignore_case(&upper, &lower));
            assert!(ordinal_ignore_case_cmp(&upper, &lower).is_eq());
            assert!(ordinal_ignore_case_cmp(&lower, &upper).is_eq());
        }
        assert!(!ordinal_ignore_case("Straße", "STRASSE"));
        assert!(ordinal_ignore_case_cmp("\u{10428}", "\u{10401}").is_lt());
        for (left, right) in [("ς", "Σ"), ("µ", "Μ"), ("ϐ", "Β"), ("ᾀ", "ᾈ")] {
            assert!(ordinal_ignore_case(left, right));
        }
        for (left, right) in [("İ", "i"), ("ı", "I"), ("ſ", "S"), ("ẞ", "ß"), ("K", "k")] {
            assert!(!ordinal_ignore_case(left, right));
        }
        assert!(ordinal_ignore_case_cmp("\u{10400}", "\u{ffff}").is_gt());
    }
}
