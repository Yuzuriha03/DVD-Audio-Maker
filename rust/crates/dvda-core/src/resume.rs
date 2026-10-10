//! Persistent evidence for complete, validated staged discs. Final publication
//! remains transactional; cancellation retains staged work for the next build.
use crate::{build::Job, disc::Disc, identity, preparation::state, signature};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Store {
    path: PathBuf,
    entries: Map<String, Value>,
}
impl Store {
    pub fn load(directory: &Path, caller: &mut dyn dvda_native::media::Callbacks) -> Self {
        let path = directory.join("resume.json");
        let entries = if path.exists() {
            match state::read_json(&path)
                .map_err(|e| e.message)
                .and_then(validate_entries)
            {
                Ok(entries) => entries,
                Err(reason) => {
                    caller.emit(2, &format!("[警告] 续跑记录无法读取，将重新出盘: {reason}"));
                    Map::new()
                }
            }
        } else {
            Map::new()
        };
        Self { path, entries }
    }
    pub fn matches(&self, disc: usize, signature: &str, iso: &Path) -> bool {
        self.entries.get(&disc.to_string()).is_some_and(|entry| {
            entry["signature"] == signature
                && state::matches(&iso.to_string_lossy(), &entry["iso"])
                && self.audit_log(disc).is_some()
        })
    }
    pub fn audit_log(&self, disc: usize) -> Option<&str> {
        let entry = self.entries.get(&disc.to_string())?;
        let log = entry["audit_log"].as_str()?;
        if entry["audit_hash"].as_str()? != crate::hash::hex_digest(log.as_bytes()) {
            return None;
        }
        let parsed = crate::verification::parse_audit_log(log);
        (!parsed.rows.is_empty() && parsed.commands.len() == 1).then_some(log)
    }
    pub fn record(
        &mut self,
        disc: usize,
        signature: &str,
        iso: &Path,
        audit_log: &str,
    ) -> Result<(), String> {
        let identity = identity::compute(&iso.to_string_lossy()).map_err(|e| e.to_string())?;
        self.entries.insert(
            disc.to_string(),
            json!({"signature":signature,"iso":identity,"audit_log":audit_log,"audit_hash":crate::hash::hex_digest(audit_log.as_bytes())}),
        );
        state::write_json(&self.path, &self.entries).map_err(|e| e.message)
    }
    pub fn discard(&mut self, disc: usize) {
        self.entries.remove(&disc.to_string());
    }
}

// Match the old typed reader: absent fields and null reference values are
// allowed, but malformed known fields invalidate the record with a warning.
// Keep unknown fields so future additions do not make old records unreadable.
fn validate_entries(value: Value) -> Result<Map<String, Value>, String> {
    if value.is_null() {
        return Ok(Map::new());
    }
    let entries = value.as_object().ok_or("Invalid resume record")?;
    let mut validated = Map::new();
    for (key, entry) in entries {
        let disc = key.parse::<i32>().map_err(|_| "Invalid resume record")?;
        if !entry.is_null() {
            let object = entry.as_object().ok_or("Invalid resume record")?;
            for key in ["signature", "created", "audit_log", "audit_hash"] {
                if object
                    .get(key)
                    .is_some_and(|v| !v.is_null() && !v.is_string())
                {
                    return Err("Invalid resume record".into());
                }
            }
            if let Some(identity) = object.get("iso").filter(|v| !v.is_null()) {
                let identity = identity.as_object().ok_or("Invalid resume record")?;
                for key in ["Size", "LastWriteUtcTicks"] {
                    if identity.get(key).is_some_and(|v| v.as_i64().is_none()) {
                        return Err("Invalid resume record".into());
                    }
                }
                for key in ["Path", "HeadHash", "TailHash"] {
                    if identity
                        .get(key)
                        .is_some_and(|v| !v.is_null() && !v.is_string())
                    {
                        return Err("Invalid resume record".into());
                    }
                }
            }
        }
        validated.insert(disc.to_string(), entry.clone());
    }
    Ok(validated)
}

pub fn disc_signature(job: &Job, disc: &Disc) -> Result<String, String> {
    let value = signature::dispatch(
        "build.disc_signature",
        json!({
            "DiscNumber":disc.number, "VolumeId":format!("{} {}",job.title,disc.number),
            "IsoName":job.iso_name(disc.number), "Title":job.title,
            "TitleMode":job.diagnostic_title_mode, "GroupLimit":job.group_track_limit,
            "AuthorPath":"rust-author", "AuthorLibraryDirectory":job.media_library.parent(),
            "MenuEnabled":job.menu_enabled, "MenuTracksPerPage":job.menu_tracks_per_page,
            "MenuIndexMinimumAlbums":job.menu_index_minimum_albums, "MenuStillPictures":job.menu_still_pictures,
            "MenuCoverDim":job.menu_cover_dim, "MenuFont":job.menu_font,
            "MenuFontJapanese":job.menu_font_japanese, "MenuFontKorean":job.menu_font_korean,
            "ImageLibraryDirectory":job.image_library.as_ref().and_then(|p| p.parent()),
            "Tracks":disc.tracks,
        }),
    )?;
    // Include the author, source audio and every file affecting menu rendering.
    let author = crate::hash::reader_digest(
        fs::File::open(std::env::current_exe().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let sources = disc
        .tracks
        .iter()
        .map(|t| identity::compute(&t.source_path).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut assets = std::collections::BTreeSet::new();
    if job.menu_enabled {
        for track in &disc.tracks {
            if let Some(directory) = Path::new(&track.source_path).parent() {
                for ext in ["jpg", "jpeg", "png", "webp"] {
                    assets.insert(directory.join(format!("cover.{ext}")));
                }
            }
        }
        for configured in [
            &job.menu_font,
            &job.menu_font_japanese,
            &job.menu_font_korean,
        ] {
            let path = PathBuf::from(configured.split('[').next().unwrap_or(configured));
            if path.is_file() {
                assets.insert(path);
            }
        }
        for directory in [
            job.menu_binary_directory.join("fonts"),
            Path::new(&job.author_source).join("menu"),
        ] {
            if directory.is_dir() {
                for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
                    let path = entry.map_err(|e| e.to_string())?.path();
                    if path.is_file() {
                        assets.insert(path);
                    }
                }
            }
        }
    }
    let assets = assets
        .into_iter()
        .map(|path| {
            let digest = if path.is_file() {
                Some(
                    crate::hash::reader_digest(fs::File::open(&path).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?,
                )
            } else {
                None
            };
            Ok((path, digest))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(crate::hash::hex_digest(
        &serde_json::to_vec(&(value, author, sources, assets, job.disc_bytes))
            .map_err(|e| e.to_string())?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Events(Vec<String>);
    impl dvda_native::media::Callbacks for Events {
        fn emit(&mut self, _: i32, text: &str) {
            self.0.push(text.into());
        }
        fn cancelled(&mut self) -> bool {
            false
        }
    }
    #[test]
    fn typed_resume_record_failures_warn_and_null_remains_empty() {
        let directory = std::env::temp_dir().join(format!(
            "dvda-resume-schema-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        for bad in [
            json!({"1":{"signature":123}}),
            json!({"1":{"signature":"expected","iso":{"Size":"bad"}}}),
            json!({"not-a-disc":{"signature":"expected"}}),
            json!({"1":{"iso":{"LastWriteUtcTicks":null}}}),
            json!({"1":{"iso":{"HeadHash":77}}}),
            json!({"1":{"audit_log":[]}}),
            json!([]),
        ] {
            fs::write(
                directory.join("resume.json"),
                serde_json::to_vec(&bad).unwrap(),
            )
            .unwrap();
            let mut events = Events::default();
            let store = Store::load(&directory, &mut events);
            assert_eq!(events.0.len(), 1, "{bad}");
            assert!(store.entries.is_empty());
            assert!(!store.matches(1, "expected", &directory.join("not-an-iso")));
        }
        for good in [
            Value::Null,
            json!({}),
            json!({"1":null}),
            json!({"01":{"signature":null,"iso":null,"future":123}}),
        ] {
            fs::write(
                directory.join("resume.json"),
                serde_json::to_vec(&good).unwrap(),
            )
            .unwrap();
            let mut events = Events::default();
            let store = Store::load(&directory, &mut events);
            assert!(events.0.is_empty(), "{good}: {:?}", events.0);
            assert!(!store.matches(1, "expected", &directory.join("not-an-iso")));
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
