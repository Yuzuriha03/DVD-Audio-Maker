use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{self, Read},
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
///
/// # Errors
/// Returns `NotFound` when neither a packaged alias nor an existing font path can be resolved.
///
/// # Panics
/// Panics only if the fixed four-region alias table exceeds the `u32` face-index range.
pub fn resolve(name: &str, runtime: &Path) -> io::Result<FontLocation> {
    let alias = name.strip_prefix("DVDA-").unwrap_or(name);
    if let Some(face_index) = ["SC", "JP", "KR", "TC"]
        .iter()
        .position(|region| alias == format!("Noto-Sans-CJK-{region}"))
    {
        return Ok(FontLocation {
            path: runtime.join("fonts").join("DvdaNotoCJK-Regular.ttc"),
            face_index: u32::try_from(face_index).expect("the packaged font has four faces"),
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

struct Entry {
    path: PathBuf,
    data: Arc<[u8]>,
    valid_faces: HashSet<u32>,
}

/// Job-scoped cache. Files are immutable for the lifetime of an image job.
#[derive(Default)]
pub struct FontCache {
    entries: VecDeque<Entry>,
    resolved: HashMap<PathBuf, PathBuf>,
    bytes: usize,
}

impl FontCache {
    /// Load, validate, and cache one font file within the job's memory budget.
    ///
    /// # Errors
    /// Returns an error for path resolution, I/O, oversized fonts, or an invalid face index.
    ///
    /// # Panics
    /// Panics only if an internal cache position no longer identifies an existing entry.
    pub fn load(&mut self, location: &FontLocation) -> io::Result<Arc<[u8]>> {
        let path = if let Some(path) = self.resolved.get(&location.path) {
            path.clone()
        } else {
            let path = location.path.canonicalize()?;
            self.resolved.insert(location.path.clone(), path.clone());
            path
        };
        if let Some(index) = self.entries.iter().position(|entry| entry.path == path) {
            let mut entry = self.entries.remove(index).expect("located cache entry");
            let data = Arc::clone(&entry.data);
            let validation = if entry.valid_faces.contains(&location.face_index) {
                Ok(())
            } else {
                validate(&data, location.face_index)
            };
            if validation.is_ok() {
                entry.valid_faces.insert(location.face_index);
            }
            self.entries.push_back(entry);
            validation?;
            return Ok(data);
        }
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
            if let Some(old) = self.entries.pop_front() {
                self.bytes -= old.data.len();
            } else {
                break;
            }
        }
        let data: Arc<[u8]> = data.into();
        self.bytes += data.len();
        self.entries.push_back(Entry {
            path,
            data: Arc::clone(&data),
            valid_faces: HashSet::from([location.face_index]),
        });
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
                assert_eq!(location.face_index, u32::try_from(face).unwrap());
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

    #[test]
    fn cache_hit_skips_only_already_validated_faces() {
        let path = std::env::temp_dir().join(format!("dvda-font-cache-{}", std::process::id()));
        fs::write(&path, b"not a real font").unwrap();
        let canonical = path.canonicalize().unwrap();
        let mut cache = FontCache::default();
        cache.entries.push_back(Entry {
            path: canonical,
            data: Arc::from(&b"not a real font"[..]),
            valid_faces: HashSet::from([0]),
        });
        let location = FontLocation {
            path: path.clone(),
            face_index: 0,
        };
        assert!(cache.load(&location).is_ok());
        let error = cache
            .load(&FontLocation {
                path: path.clone(),
                face_index: 1,
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.resolved.len(), 1);
        fs::remove_file(path).unwrap();
    }

    fn legacy_hit(
        entries: &mut VecDeque<(PathBuf, Arc<[u8]>)>,
        location: &FontLocation,
    ) -> io::Result<Arc<[u8]>> {
        let path = location.path.canonicalize()?;
        let index = entries.iter().position(|entry| entry.0 == path).unwrap();
        let entry = entries.remove(index).unwrap();
        let data = Arc::clone(&entry.1);
        entries.push_back(entry);
        validate(&data, location.face_index)?;
        Ok(data)
    }

    #[test]
    #[ignore = "deterministic font cache hit throughput benchmark"]
    fn font_cache_hit_throughput() {
        use std::{hint::black_box, time::Instant};

        const HITS: usize = 500;
        let location = FontLocation {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../build/font-sources/DvdaNotoCJK-Regular.ttc"),
            face_index: 2,
        };
        assert!(location.path.is_file(), "real TTC fixture is required");
        let mut cache = FontCache::default();
        let expected = cache.load(&location).unwrap();
        let canonical = location.path.canonicalize().unwrap();
        let mut legacy = VecDeque::from([(canonical, Arc::clone(&expected))]);
        let mut legacy_times = Vec::new();
        let mut cached_times = Vec::new();
        for round in 0..6 {
            if round % 2 == 0 {
                let started = Instant::now();
                for _ in 0..HITS {
                    black_box(legacy_hit(&mut legacy, &location).unwrap());
                }
                legacy_times.push(started.elapsed());
                let started = Instant::now();
                for _ in 0..HITS {
                    black_box(cache.load(&location).unwrap());
                }
                cached_times.push(started.elapsed());
            } else {
                let started = Instant::now();
                for _ in 0..HITS {
                    black_box(cache.load(&location).unwrap());
                }
                cached_times.push(started.elapsed());
                let started = Instant::now();
                for _ in 0..HITS {
                    black_box(legacy_hit(&mut legacy, &location).unwrap());
                }
                legacy_times.push(started.elapsed());
            }
        }
        for (round, (legacy, cached)) in legacy_times.iter().zip(&cached_times).enumerate() {
            println!(
                "[METRIC] font_round={} legacy_ms={:.4} cached_ms={:.4}",
                round + 1,
                legacy.as_secs_f64() * 1000.0,
                cached.as_secs_f64() * 1000.0
            );
        }
        assert!(Arc::ptr_eq(&expected, &cache.load(&location).unwrap()));
    }
}
