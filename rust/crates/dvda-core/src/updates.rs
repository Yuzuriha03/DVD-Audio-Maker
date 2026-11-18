//! Shared version and release-asset selection for the desktop and mobile apps.
//!
//! The hosts fetch the same HTTPS endpoint with their platform transport. This
//! module decides whether to offer an update and never downloads or opens files.

use serde::Deserialize;
use std::fmt;

pub const RELEASE_API_URL: &str =
    "https://api.github.com/repos/Yuzuriha03/DVD-Audio-Maker/releases/latest";

const DOWNLOAD_ROOT: &str = "https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/download";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    WindowsX64,
    WindowsArm64,
    AndroidArm64,
}

impl Platform {
    const fn asset_suffix(self) -> &'static str {
        match self {
            Self::WindowsX64 => "win-x64.exe",
            Self::WindowsArm64 => "win-arm64.exe",
            Self::AndroidArm64 => "android-arm64.apk",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    pub download_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateError {
    InvalidCurrentVersion,
    InvalidResponse,
    InvalidReleaseVersion,
    InvalidAsset,
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCurrentVersion => "Invalid current product version",
            Self::InvalidResponse => "Invalid release response",
            Self::InvalidReleaseVersion => "Invalid release version",
            Self::InvalidAsset => "Invalid or ambiguous release download",
        })
    }
}

impl std::error::Error for UpdateError {}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    state: String,
    browser_download_url: String,
}

/// Parse the release convention (e.g. v1.2) and the packaged version name
/// (e.g. 1.2.0) without treating a trailing zero as a newer release.
fn version_number(text: &str) -> Option<[u32; 3]> {
    fn component(part: &str) -> Option<u32> {
        if !part.bytes().all(|b| b.is_ascii_digit()) || (part.len() > 1 && part.starts_with('0')) {
            return None;
        }
        part.parse().ok()
    }

    let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
    let version = [
        component(parts.next()?)?,
        component(parts.next()?)?,
        component(parts.next().unwrap_or("0"))?,
    ];
    parts.next().is_none().then_some(version)
}

/// Select a newer stable release with an uploaded asset for this platform.
///
/// Returns no update for equal or older releases, drafts, previews, or a release
/// that has no uploaded package for this platform. Accept only the exact HTTPS
/// direct-download URL belonging to the pinned repository, tag and asset name.
///
/// # Errors
/// Returns an error for invalid current or release metadata, or an ambiguous or
/// unexpected download URL. Callers may silently ignore startup check failures.
pub fn select_update(
    current_version: &str,
    response_json: &str,
    platform: Platform,
) -> Result<Option<Update>, UpdateError> {
    let current = version_number(current_version).ok_or(UpdateError::InvalidCurrentVersion)?;
    if response_json.len() > 1024 * 1024 {
        return Err(UpdateError::InvalidResponse);
    }
    let release: Release =
        serde_json::from_str(response_json).map_err(|_| UpdateError::InvalidResponse)?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let latest = version_number(&release.tag_name).ok_or(UpdateError::InvalidReleaseVersion)?;
    if latest <= current {
        return Ok(None);
    }

    let file = format!(
        "DVD-Audio-Maker-{}-{}",
        release.tag_name,
        platform.asset_suffix()
    );
    let mut candidates = release
        .assets
        .iter()
        .filter(|asset| asset.name == file && asset.state == "uploaded");
    let Some(asset) = candidates.next() else {
        return Ok(None);
    };
    let expected = format!("{DOWNLOAD_ROOT}/{}/{file}", release.tag_name);
    if asset.browser_download_url != expected || candidates.next().is_some() {
        return Err(UpdateError::InvalidAsset);
    }
    Ok(Some(Update {
        version: release.tag_name,
        download_url: expected,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn asset(tag: &str, suffix: &str) -> Value {
        let name = format!("DVD-Audio-Maker-{tag}-{suffix}");
        json!({
            "name": name,
            "state": "uploaded",
            "browser_download_url": format!("{DOWNLOAD_ROOT}/{tag}/{name}"),
        })
    }

    fn release(tag: &str) -> Value {
        json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": false,
            "assets": [
                asset(tag, "android-arm64.apk"),
                asset(tag, "win-arm64.exe"),
                asset(tag, "win-x64.exe"),
            ],
        })
    }

    #[test]
    fn same_tag_does_not_offer_an_update_when_assets_were_replaced() {
        let same = release("v1.2").to_string();
        for current in ["1.2", "1.2.0", "v1.2.0", "v1.2.1"] {
            assert_eq!(
                select_update(current, &same, Platform::WindowsX64),
                Ok(None)
            );
        }
    }

    #[test]
    fn compares_numerically_and_selects_only_the_correct_direct_link() {
        let latest = release("v1.10").to_string();
        for (platform, suffix) in [
            (Platform::WindowsX64, "win-x64.exe"),
            (Platform::WindowsArm64, "win-arm64.exe"),
            (Platform::AndroidArm64, "android-arm64.apk"),
        ] {
            let found = select_update("1.9.7", &latest, platform).unwrap().unwrap();
            assert_eq!(found.version, "v1.10");
            assert_eq!(
                found.download_url,
                format!("{DOWNLOAD_ROOT}/v1.10/DVD-Audio-Maker-v1.10-{suffix}")
            );
        }
        assert!(
            select_update(
                "1.2.0",
                &release("v1.2.1").to_string(),
                Platform::WindowsX64
            )
            .unwrap()
            .is_some()
        );
    }

    #[test]
    fn drafts_previews_and_missing_packages_do_not_produce_notifications() {
        let mut candidate = release("v9.0-rc.1");
        candidate["prerelease"] = json!(true);
        assert_eq!(
            select_update("1.2.0", &candidate.to_string(), Platform::WindowsX64),
            Ok(None)
        );
        candidate = release("v9.0");
        candidate["draft"] = json!(true);
        assert_eq!(
            select_update("1.2.0", &candidate.to_string(), Platform::WindowsX64),
            Ok(None)
        );
        candidate["draft"] = json!(false);
        assert_eq!(
            select_update("1.2.0", &candidate.to_string(), Platform::WindowsX64)
                .unwrap()
                .unwrap()
                .version,
            "v9.0"
        );
        candidate["assets"][2]["state"] = json!("starter");
        assert_eq!(
            select_update("1.2.0", &candidate.to_string(), Platform::WindowsX64),
            Ok(None)
        );
        candidate["assets"] = json!([asset("v9.0", "android-arm64.apk")]);
        assert_eq!(
            select_update("1.2.0", &candidate.to_string(), Platform::WindowsArm64),
            Ok(None)
        );
    }

    #[test]
    fn refuses_malformed_versions_and_untrusted_or_ambiguous_urls() {
        let mut r = release("v1.3");
        assert_eq!(
            select_update("1.2.0", "<html>", Platform::WindowsX64),
            Err(UpdateError::InvalidResponse)
        );
        assert_eq!(
            select_update("not-installed", &r.to_string(), Platform::WindowsX64),
            Err(UpdateError::InvalidCurrentVersion)
        );
        for tag in [
            "v1",
            "v1.3.0-preview",
            "v1.03",
            "v1.3.0/../x",
            "v-1.3",
            "v99999999999.3",
        ] {
            assert_eq!(
                select_update("1.2.0", &release(tag).to_string(), Platform::WindowsX64),
                Err(UpdateError::InvalidReleaseVersion)
            );
        }
        for url in [
            "https://github.com.evil.invalid/Yuzuriha03/DVD-Audio-Maker/releases/download/v1.3/DVD-Audio-Maker-v1.3-win-x64.exe",
            "https://github.com/another-owner/DVD-Audio-Maker/releases/download/v1.3/DVD-Audio-Maker-v1.3-win-x64.exe",
            "http://github.com/Yuzuriha03/DVD-Audio-Maker/releases/download/v1.3/DVD-Audio-Maker-v1.3-win-x64.exe",
            "https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/download/v1.3/DVD-Audio-Maker-v1.3-win-arm64.exe",
        ] {
            r["assets"][2]["browser_download_url"] = json!(url);
            assert_eq!(
                select_update("1.2.0", &r.to_string(), Platform::WindowsX64),
                Err(UpdateError::InvalidAsset)
            );
        }
        r["assets"] = json!([asset("v1.3", "win-x64.exe"), asset("v1.3", "win-x64.exe")]);
        assert_eq!(
            select_update("1.2.0", &r.to_string(), Platform::WindowsX64),
            Err(UpdateError::InvalidAsset)
        );
    }
}
