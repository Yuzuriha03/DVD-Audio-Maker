use std::{
    collections::VecDeque,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
const CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontLocation {
    pub path: PathBuf,
    pub face_index: u32,
}

/// Resolve the aliases emitted by the runtime packager without a Magick typemap.
/// Arbitrary family-name discovery belongs to the application's font selector.
pub fn resolve(name: &str, runtime: &Path) -> io::Result<FontLocation> {
    let alias = name.strip_prefix("DVDA-").unwrap_or(name);
    if let Some(face_index) = ["SC", "JP", "KR", "TC"]
        .iter()
        .position(|region| alias == format!("Noto-Sans-CJK-{region}"))
    {
        return Ok(FontLocation {
            path: runtime.join("fonts").join("DvdaNotoCJK-Regular.ttc"),
            face_index: face_index as u32,
        });
    }
    let path = PathBuf::from(name);
    if path.is_file() {
        return Ok(FontLocation {
            path,
            face_index: 0,
        });
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("unresolved font: {name}"),
    ))
}

/// Job-scoped cache. Files are immutable for the lifetime of an image job.
#[derive(Default)]
pub struct FontCache {
    entries: VecDeque<(PathBuf, Arc<[u8]>)>,
    bytes: usize,
}

impl FontCache {
    pub fn load(&mut self, location: &FontLocation) -> io::Result<Arc<[u8]>> {
        let path = location.path.canonicalize()?;
        if let Some(index) = self.entries.iter().position(|entry| entry.0 == path) {
            let entry = self.entries.remove(index).expect("located cache entry");
            let data = Arc::clone(&entry.1);
            self.entries.push_back(entry);
            validate(&data, location.face_index)?;
            return Ok(data);
        }
        use std::io::Read;
        let file = fs::File::open(&path)?;
        if file.metadata()?.len() > MAX_FONT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "font exceeds 64 MiB",
            ));
        }
        let mut data = Vec::new();
        file.take(MAX_FONT_BYTES + 1).read_to_end(&mut data)?;
        if data.len() as u64 > MAX_FONT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "font exceeds 64 MiB",
            ));
        }
        validate(&data, location.face_index)?;
        while self.bytes + data.len() > CACHE_BYTES {
            if let Some((_, old)) = self.entries.pop_front() {
                self.bytes -= old.len();
            } else {
                break;
            }
        }
        let data: Arc<[u8]> = data.into();
        self.bytes += data.len();
        self.entries.push_back((path, Arc::clone(&data)));
        Ok(data)
    }
}

fn validate(data: &[u8], face_index: u32) -> io::Result<()> {
    rustybuzz::Face::from_slice(data, face_index)
        .map(|_| ())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid font collection face"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packaged_aliases_keep_region_face_order() {
        for (face, region) in ["SC", "JP", "KR", "TC"].iter().enumerate() {
            for prefix in ["DVDA-", ""] {
                let location = resolve(
                    &format!("{prefix}Noto-Sans-CJK-{region}"),
                    Path::new("runtime"),
                )
                .unwrap();
                assert_eq!(location.face_index, face as u32);
                assert_eq!(
                    location.path,
                    Path::new("runtime")
                        .join("fonts")
                        .join("DvdaNotoCJK-Regular.ttc")
                );
            }
        }
        assert!(resolve("DVDA-Noto-Sans-CJK-unknown", Path::new("runtime")).is_err());
        assert!(validate(&[], 0).is_err());
    }
}
