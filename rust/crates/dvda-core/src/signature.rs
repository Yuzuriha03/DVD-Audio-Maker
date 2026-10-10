//! Deterministic per-disc resume signatures.
use crate::{hash, identity};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Cache identity includes the code that converts PCM, its native dependencies,
/// the codec, and the exact optional metadata context. Full hashes also detect
/// replacements that preserve file size and timestamps.
pub fn encoding_identity(
    policy: &str,
    media: &Path,
    encoder: Option<&Path>,
    metadata_context: &str,
) -> Result<String, String> {
    let file_hash = |path: &Path| {
        identity::open_read(&path.to_string_lossy())
            .and_then(hash::reader_digest)
            .map_err(|error| format!("{}: {error}", path.display()))
    };
    let mut components = std::collections::BTreeMap::new();
    components.insert("media".to_owned(), file_hash(media)?);
    #[cfg(feature = "direct-bridges")]
    components.insert(
        "linked-media-executable".to_owned(),
        file_hash(&std::env::current_exe().map_err(|error| error.to_string())?)?,
    );
    let dependency_directories: Vec<PathBuf> =
        media.parent().map(Path::to_owned).into_iter().collect();
    #[cfg(feature = "direct-bridges")]
    let dependency_directories = {
        let mut directories = dependency_directories;
        if let Some(directory) = crate::runtime::directory()
            && !directories.iter().any(|path| path == directory)
        {
            directories.push(directory.to_owned());
        }
        directories
    };
    let mut dependencies = Vec::new();
    for directory in &dependency_directories {
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
            {
                dependencies.push(path);
            }
        }
    }
    #[cfg(feature = "direct-bridges")]
    {
        dependencies.extend(media_loaded_dependencies(
            &dependencies,
            dvda_native::media::loaded_dependencies().map_err(|error| error.to_string())?,
        ));
    }
    add_dependency_hashes(&mut components, dependencies, &file_hash)?;
    #[cfg(feature = "rust-mlp")]
    components.insert(
        "linked-encoder-executable".to_owned(),
        file_hash(&std::env::current_exe().map_err(|error| error.to_string())?)?,
    );
    #[cfg(not(feature = "rust-mlp"))]
    if let Some(encoder) = encoder {
        components.insert("encoder".into(), file_hash(encoder)?);
    }
    #[cfg(feature = "rust-mlp")]
    let _ = encoder;
    let context = if metadata_context.is_empty() {
        "generated-from-pcm-v1".into()
    } else {
        file_hash(Path::new(metadata_context))?
    };
    let canonical = serde_json::to_vec(&("rust-pcm-policy-v1", policy, components, context))
        .map_err(|error| error.to_string())?;
    Ok(hash::hex_digest(&canonical))
}

#[cfg(any(feature = "direct-bridges", test))]
fn media_loaded_dependencies(known: &[PathBuf], loaded: Vec<PathBuf>) -> Vec<PathBuf> {
    let names: std::collections::BTreeSet<_> = known
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .collect();
    loaded
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| names.contains(&name.to_string_lossy().to_ascii_lowercase()))
        })
        .collect()
}

fn add_dependency_hashes(
    components: &mut std::collections::BTreeMap<String, String>,
    mut paths: Vec<PathBuf>,
    file_hash: &impl Fn(&Path) -> Result<String, String>,
) -> Result<(), String> {
    paths.sort();
    paths.dedup();
    for (index, path) in paths.iter().enumerate() {
        components.insert(
            format!(
                "media-dependency:{index}:{}",
                path.file_name().unwrap().to_string_lossy().to_lowercase()
            ),
            file_hash(path)?,
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Request {
    disc_number: i32,
    volume_id: String,
    iso_name: String,
    title: String,
    title_mode: String,
    group_limit: i32,
    author_library_directory: Option<String>,
    menu_enabled: bool,
    menu_tracks_per_page: i32,
    menu_index_minimum_albums: i32,
    menu_still_pictures: bool,
    menu_cover_dim: i32,
    menu_font: String,
    menu_font_japanese: String,
    menu_font_korean: String,
    image_library_directory: Option<String>,
    tracks: Vec<Track>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Track {
    title: String,
    manifest_name: String,
    sample_rate: i32,
    bits: i32,
    mlp_source: String,
    channels: Option<i32>,
    channel_mask: Option<u32>,
    mlp_path: String,
    mlp_size: i64,
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    if operation != "build.disc_signature" {
        return Err(format!("Unsupported signature operation: {operation}"));
    }
    let request: Request = serde_json::from_value(request)
        .map_err(|error| format!("Invalid disc signature request: {error}"))?;
    Ok(Value::String(build(request)?))
}

fn build(request: Request) -> Result<String, String> {
    let mut canonical = String::new();
    line(&mut canonical, "disc", request.disc_number);
    line(&mut canonical, "volid", request.volume_id);
    line(&mut canonical, "iso", request.iso_name);
    line(&mut canonical, "title", request.title);
    line(&mut canonical, "title_mode", request.title_mode);
    line(&mut canonical, "group_limit", request.group_limit);
    let author_identity = hash::reader_digest(
        fs::File::open(std::env::current_exe().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    line(
        &mut canonical,
        "author",
        format!("rust-author|{author_identity}"),
    );
    canonical.push_str("iso_writer=rust-iso9660-udf102-v1\n");
    append_library_hashes(
        &mut canonical,
        "author_library",
        request.author_library_directory.as_deref(),
        "dll",
    )?;
    line(
        &mut canonical,
        "menu",
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            format_bool(request.menu_enabled),
            request.menu_tracks_per_page,
            request.menu_index_minimum_albums,
            format_bool(request.menu_still_pictures),
            request.menu_cover_dim,
            request.menu_font,
            request.menu_font_japanese,
            request.menu_font_korean
        ),
    );
    if request.menu_enabled {
        append_library_hashes(
            &mut canonical,
            "image",
            request.image_library_directory.as_deref(),
            "dll_or_xml",
        )?;
    }
    for track in request.tracks {
        let identity = identity::compute(&track.mlp_path).ok();
        canonical.push_str("track=");
        canonical.push_str(&track.title);
        canonical.push('|');
        canonical.push_str(&track.manifest_name);
        canonical.push('|');
        canonical.push_str(&track.sample_rate.to_string());
        canonical.push('|');
        canonical.push_str(&track.bits.to_string());
        canonical.push('|');
        canonical.push_str(&track.mlp_source);
        canonical.push('|');
        append_optional(&mut canonical, track.channels);
        canonical.push('|');
        append_optional(&mut canonical, track.channel_mask);
        canonical.push('|');
        canonical.push_str(&track.mlp_path);
        canonical.push('|');
        canonical.push_str(&track.mlp_size.to_string());
        canonical.push('|');
        if let Some(identity) = identity {
            append_identity(&mut canonical, &identity)?;
        } else {
            canonical.push_str("missing");
        }
        canonical.push('\n');
    }
    Ok(hash::hex_digest(canonical.as_bytes()))
}

fn line<T: ToString>(output: &mut String, key: &str, value: T) {
    output.push_str(key);
    output.push('=');
    output.push_str(&value.to_string());
    output.push('\n');
}

fn format_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn append_optional<T: ToString>(output: &mut String, value: Option<T>) {
    if let Some(value) = value {
        output.push_str(&value.to_string());
    }
}

fn append_identity(output: &mut String, value: &Value) -> Result<(), String> {
    let size = value
        .get("Size")
        .and_then(Value::as_u64)
        .ok_or("Missing MLP identity size")?;
    let ticks = value
        .get("LastWriteUtcTicks")
        .and_then(Value::as_i64)
        .ok_or("Missing MLP identity timestamp")?;
    let head = value
        .get("HeadHash")
        .and_then(Value::as_str)
        .ok_or("Missing MLP identity head hash")?;
    let tail = value
        .get("TailHash")
        .and_then(Value::as_str)
        .ok_or("Missing MLP identity tail hash")?;
    output.push_str(&size.to_string());
    output.push('|');
    output.push_str(&ticks.to_string());
    output.push('|');
    output.push_str(head);
    output.push('|');
    output.push_str(tail);
    Ok(())
}

fn append_library_hashes(
    output: &mut String,
    label: &str,
    directory: Option<&str>,
    kind: &str,
) -> Result<(), String> {
    let Some(directory) = directory.filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let mut paths = Vec::<PathBuf>::new();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Read signature library directory: {error}")),
    };
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if !path.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let include = match kind {
            "dll" => extension.eq_ignore_ascii_case("dll"),
            "dll_or_xml" => {
                extension.eq_ignore_ascii_case("dll") || extension.eq_ignore_ascii_case("xml")
            }
            _ => false,
        };
        if include {
            paths.push(path);
        }
    }
    paths.sort_by(|left, right| {
        left.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .cmp(&right.file_name().unwrap_or_default().to_string_lossy())
    });
    for path in paths {
        let file = fs::File::open(&path).map_err(|error| error.to_string())?;
        let digest = hash::reader_digest(file).map_err(|error| error.to_string())?;
        line(
            output,
            label,
            format!(
                "{}|{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                digest
            ),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loaded_dependencies_are_limited_to_known_media_files() {
        let known = vec![PathBuf::from(r"C:\media\avcodec.dll")];
        let loaded = vec![
            PathBuf::from(r"C:\media\avcodec.dll"),
            PathBuf::from(r"C:\Windows\System32\user32.dll"),
        ];
        assert_eq!(
            media_loaded_dependencies(&known, loaded),
            vec![PathBuf::from(r"C:\media\avcodec.dll")]
        );
    }

    #[test]
    fn same_named_dependencies_are_not_overwritten() {
        let first = PathBuf::from(r"C:\runtime-one\codec.dll");
        let second = PathBuf::from(r"C:\runtime-two\codec.dll");
        let digest = |path: &Path| Ok(path.to_string_lossy().into_owned());
        let mut components = std::collections::BTreeMap::new();
        add_dependency_hashes(
            &mut components,
            vec![second.clone(), first.clone(), first.clone()],
            &digest,
        )
        .unwrap();
        assert_eq!(components.len(), 2);
        assert!(
            components
                .values()
                .any(|value| value == &first.to_string_lossy())
        );
        assert!(
            components
                .values()
                .any(|value| value == &second.to_string_lossy())
        );
        let mut reordered = std::collections::BTreeMap::new();
        add_dependency_hashes(&mut reordered, vec![first, second], &digest).unwrap();
        assert_eq!(components, reordered);
    }

    #[test]
    fn encoding_cache_identity_detects_equal_size_equal_timestamp_replacements() {
        let root = std::env::temp_dir().join(format!(
            "dvda-codec-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let media = root.join("media.dll");
        let encoder = root.join("encoder.bin");
        let context = root.join("context.bin");
        let dependency = root.join("codec.dll");
        for path in [&media, &encoder, &context, &dependency] {
            fs::write(path, b"ABCD").unwrap();
        }
        let compute = || {
            encoding_identity("policy", &media, Some(&encoder), context.to_str().unwrap()).unwrap()
        };
        let mut before = compute();
        #[cfg(feature = "rust-mlp")]
        let paths = [&media, &context, &dependency];
        #[cfg(not(feature = "rust-mlp"))]
        let paths = [&media, &encoder, &context, &dependency];
        for path in paths {
            let modified = fs::metadata(path).unwrap().modified().unwrap();
            fs::write(path, b"ABCE").unwrap();
            fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(modified)
                .unwrap();
            let after = compute();
            assert_ne!(before, after, "{}", path.display());
            before = after;
        }
        assert_ne!(
            before,
            encoding_identity(
                "new policy",
                &media,
                Some(&encoder),
                context.to_str().unwrap()
            )
            .unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_and_same_length_library_changes_affect_signature() {
        let root = std::env::temp_dir().join(format!(
            "dvda-signature-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("directory");
        let author = root.join("author.exe");
        let mlp = root.join("track.mlp");
        let menu = root.join("menu");
        fs::create_dir_all(&menu).expect("menu");
        fs::write(&author, [1, 2, 3]).expect("author");
        fs::write(&mlp, [4, 5, 6]).expect("mlp");
        fs::write(menu.join("dvda-menu.dll"), [7, 8, 9]).expect("library");
        let request = json!({
            "DiscNumber":1,"VolumeId":"Title 1","IsoName":"Title_1.iso",
            "Title":"Title","TitleMode":"album","GroupLimit":99,
            "AuthorPath":author,"AuthorLibraryDirectory":menu,
            "MenuEnabled":true,"MenuTracksPerPage":12,"MenuIndexMinimumAlbums":4,
            "MenuStillPictures":true,"MenuCoverDim":35,"MenuFont":"font",
            "MenuFontJapanese":"","MenuFontKorean":"","ImageLibraryDirectory":menu,
            "Tracks":[{
                "Title":"Track","ManifestName":"01","SampleRate":48000,"Bits":24,
                "MlpSource":"external","Channels":2,"ChannelMask":3,
                "MlpPath":mlp,"MlpSize":3
            }]
        });
        let first = build(serde_json::from_value(request.clone()).unwrap()).expect("signature");
        fs::write(menu.join("dvda-menu.dll"), [7, 8, 0]).expect("library");
        let second = build(serde_json::from_value(request).unwrap()).expect("signature");
        assert_ne!(first, second);
        let _ = fs::remove_dir_all(root);
    }
}
