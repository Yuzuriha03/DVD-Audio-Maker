//! Task-scoped decoded and processed image sharing with bounded memory.
use crate::{
    geometry::{Dimensions, ResizeMode},
    raster::{MAX_WORKING_BYTES, Raster},
};
use std::{
    collections::{HashMap, VecDeque, hash_map::DefaultHasher},
    fs::File,
    hash::{Hash, Hasher},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::UNIX_EPOCH,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    size: u64,
    modified: u128,
    head: u64,
    tail: u64,
}

fn file_identity(path: &Path) -> std::io::Result<FileIdentity> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut file = File::open(path)?;
    let head_bytes = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    let mut head = vec![0; 4096usize.min(head_bytes)];
    file.read_exact(&mut head)?;
    let mut tail = vec![0; head.len()];
    file.seek(SeekFrom::Start(
        metadata.len().saturating_sub(tail.len() as u64),
    ))?;
    file.read_exact(&mut tail)?;
    let hash = |bytes: &[u8]| {
        let mut hash = DefaultHasher::new();
        bytes.hash(&mut hash);
        hash.finish()
    };
    Ok(FileIdentity {
        size: metadata.len(),
        modified,
        head: hash(&head),
        tail: hash(&tail),
    })
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Resize {
    width: u32,
    height: u32,
    mode: u8,
}

impl Resize {
    fn new(size: Dimensions, mode: ResizeMode) -> Self {
        Self {
            width: size.width(),
            height: size.height(),
            mode: match mode {
                ResizeMode::Fit => 0,
                ResizeMode::Fill => 1,
                ResizeMode::Exact => 2,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Operation {
    Decode,
    Resize {
        steps: Vec<Resize>,
        extent: Option<(u32, u32)>,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Key {
    path: PathBuf,
    operation: Operation,
}

struct Entry {
    identity: FileIdentity,
    image: Arc<Raster>,
}

#[derive(Default)]
struct State {
    entries: HashMap<Key, Entry>,
    lru: VecDeque<Key>,
    bytes: usize,
}

impl State {
    fn lookup(&mut self, key: &Key, identity: FileIdentity) -> Option<Arc<Raster>> {
        let image = match self.entries.get(key) {
            Some(entry) if entry.identity == identity => Arc::clone(&entry.image),
            Some(_) => {
                self.remove(key);
                return None;
            }
            None => return None,
        };
        self.lru.retain(|cached| cached != key);
        self.lru.push_back(key.clone());
        Some(image)
    }

    fn remove(&mut self, key: &Key) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes = self.bytes.saturating_sub(entry.image.rgba().len());
            self.lru.retain(|cached| cached != key);
        }
    }

    fn evict_oldest(&mut self) {
        let Some(key) = self.lru.pop_front() else {
            return;
        };
        if let Some(entry) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(entry.image.rgba().len());
        }
    }

    fn insert(&mut self, key: Key, identity: FileIdentity, image: Arc<Raster>, max_bytes: usize) {
        let bytes = image.rgba().len();
        if bytes > max_bytes {
            return;
        }
        self.remove(&key);
        while self.bytes.saturating_add(bytes) > max_bytes {
            self.evict_oldest();
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.lru.push_back(key.clone());
        self.entries.insert(key, Entry { identity, image });
    }
}

pub struct ImageCache {
    state: Mutex<State>,
}

impl std::fmt::Debug for ImageCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.lock();
        formatter
            .debug_struct("ImageCache")
            .field("entries", &state.entries.len())
            .field("bytes", &state.bytes)
            .finish()
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }
}

impl ImageCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lookup(&self, key: &Key, identity: FileIdentity) -> Option<Arc<Raster>> {
        self.lock().lookup(key, identity)
    }

    fn insert(&self, key: Key, identity: FileIdentity, image: Arc<Raster>) -> Arc<Raster> {
        self.lock()
            .insert(key, identity, Arc::clone(&image), MAX_WORKING_BYTES);
        image
    }

    /// Decode and cache an image using its current file identity.
    ///
    /// # Errors
    /// Propagates file identity, image decoding, allocation, and I/O failures.
    pub fn decode(&self, path: &Path) -> std::io::Result<Arc<Raster>> {
        self.decode_with_identity(path, file_identity(path)?)
    }

    fn decode_with_identity(
        &self,
        path: &Path,
        identity: FileIdentity,
    ) -> std::io::Result<Arc<Raster>> {
        let key = Key {
            path: path.to_path_buf(),
            operation: Operation::Decode,
        };
        if let Some(image) = self.lookup(&key, identity) {
            return Ok(image);
        }
        let image = Arc::new(crate::codec::decode(BufReader::new(File::open(path)?))?);
        Ok(self.insert(key, identity, image))
    }

    /// Resize and cache an image using the requested geometry and centering policy.
    ///
    /// # Errors
    /// Propagates file identity, image decoding, resizing, allocation, and I/O failures.
    pub fn resized(
        &self,
        path: &Path,
        size: Dimensions,
        mode: ResizeMode,
        centered: bool,
    ) -> std::io::Result<Arc<Raster>> {
        self.resized_chain(path, &[(size, mode)], centered.then_some(size))
    }

    /// Apply ordered resizes and an optional centered extent, caching only the final result.
    /// The decoded source is shared with other operations using the same file identity.
    ///
    /// # Errors
    /// Propagates file identity, image decoding, resizing, allocation, and I/O failures.
    pub fn resized_chain(
        &self,
        path: &Path,
        steps: &[(Dimensions, ResizeMode)],
        extent: Option<Dimensions>,
    ) -> std::io::Result<Arc<Raster>> {
        let identity = file_identity(path)?;
        let key = Key {
            path: path.to_path_buf(),
            operation: Operation::Resize {
                steps: steps
                    .iter()
                    .map(|(size, mode)| Resize::new(*size, *mode))
                    .collect(),
                extent: extent.map(|size| (size.width(), size.height())),
            },
        };
        if let Some(image) = self.lookup(&key, identity) {
            return Ok(image);
        }
        let source = self.decode_with_identity(path, identity)?;
        let mut processed: Option<Raster> = None;
        for (size, mode) in steps {
            let current = processed.as_ref().unwrap_or(source.as_ref());
            let resized = current
                .dimensions()
                .resized(*size, *mode)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
            if resized != current.dimensions() {
                processed = Some(current.resized(*size, *mode)?);
            }
        }
        if let Some(size) = extent {
            let current = processed.as_ref().unwrap_or(source.as_ref());
            if current.dimensions() != size {
                processed = Some(current.centered_extent(size, [0, 0, 0, 255])?);
            }
        }
        let Some(image) = processed else {
            return Ok(source);
        };
        Ok(self.insert(key, identity, Arc::new(image)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn shares_images_and_invalidates_changed_sources() {
        let root = std::env::temp_dir().join(format!(
            "dvda-image-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("cover.png");
        let image = Raster::solid(Dimensions::new(8, 8).unwrap(), [1, 2, 3, 255]).unwrap();
        crate::codec::write_png(&image, File::create(&path).unwrap()).unwrap();
        let cache = ImageCache::default();
        let first = cache.decode(&path).unwrap();
        let second = cache.decode(&path).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let resized = cache
            .resized(
                &path,
                Dimensions::new(4, 4).unwrap(),
                ResizeMode::Fill,
                true,
            )
            .unwrap();
        assert_eq!(resized.dimensions(), Dimensions::new(4, 4).unwrap());
        let again = cache
            .resized(
                &path,
                Dimensions::new(4, 4).unwrap(),
                ResizeMode::Fill,
                true,
            )
            .unwrap();
        assert!(Arc::ptr_eq(&resized, &again));
        let mut file = File::create(&path).unwrap();
        crate::codec::write_png(
            &Raster::solid(Dimensions::new(8, 8).unwrap(), [9, 8, 7, 255]).unwrap(),
            &mut file,
        )
        .unwrap();
        file.flush().unwrap();
        drop(file);
        let changed = cache.decode(&path).unwrap();
        assert_eq!(changed.rgba()[..4], [9, 8, 7, 255]);
        let changed_resized = cache
            .resized(
                &path,
                Dimensions::new(4, 4).unwrap(),
                ResizeMode::Fill,
                true,
            )
            .unwrap();
        assert_eq!(changed_resized.rgba()[..4], [9, 8, 7, 255]);
        assert!(!Arc::ptr_eq(&resized, &changed_resized));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resize_chain_matches_direct_pixels_and_only_caches_the_final_result() {
        let root = std::env::temp_dir().join(format!(
            "dvda-image-chain-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("cover.png");
        let size = Dimensions::new(5, 3).unwrap();
        let pixels = (0i32..15).flat_map(i32::to_le_bytes).collect();
        let source = Raster::from_rgba(size, pixels).unwrap();
        crate::codec::write_png(&source, File::create(&path).unwrap()).unwrap();
        let intermediate = Dimensions::new(6, 4).unwrap();
        let frame = Dimensions::new(4, 4).unwrap();
        let expected = source
            .resized(intermediate, ResizeMode::Exact)
            .unwrap()
            .resized(frame, ResizeMode::Fit)
            .unwrap()
            .centered_extent(frame, [0, 0, 0, 255])
            .unwrap();
        let cache = ImageCache::default();
        let decoded = cache.decode(&path).unwrap();
        let actual = cache
            .resized_chain(
                &path,
                &[(intermediate, ResizeMode::Exact), (frame, ResizeMode::Fit)],
                Some(frame),
            )
            .unwrap();
        assert_eq!(actual.rgba(), expected.rgba());
        let again = cache
            .resized_chain(
                &path,
                &[(intermediate, ResizeMode::Exact), (frame, ResizeMode::Fit)],
                Some(frame),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&actual, &again));
        assert_eq!(cache.lock().entries.len(), 2);
        let unchanged = cache
            .resized_chain(&path, &[(size, ResizeMode::Exact)], Some(size))
            .unwrap();
        assert!(Arc::ptr_eq(&decoded, &unchanged));
        assert_eq!(cache.lock().entries.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_hit_updates_recency_and_eviction_is_lru() {
        let identity = FileIdentity {
            size: 1,
            modified: 2,
            head: 3,
            tail: 4,
        };
        let key = |name: &str| Key {
            path: PathBuf::from(name),
            operation: Operation::Decode,
        };
        let image =
            || Arc::new(Raster::solid(Dimensions::new(2, 2).unwrap(), [1, 2, 3, 255]).unwrap());
        let a = key("a");
        let b = key("b");
        let c = key("c");
        let d = key("d");
        let a_image = image();
        let mut state = State::default();
        state.insert(a.clone(), identity, Arc::clone(&a_image), 48);
        state.insert(b.clone(), identity, image(), 48);
        state.insert(c.clone(), identity, image(), 48);
        assert_eq!(state.bytes, 48);

        let hit = state.lookup(&a, identity).unwrap();
        assert!(Arc::ptr_eq(&hit, &a_image));
        state.insert(d.clone(), identity, image(), 48);

        assert_eq!(state.entries.len(), 3);
        assert_eq!(state.lru.len(), 3);
        assert_eq!(state.bytes, 48);
        assert!(!state.entries.contains_key(&b));
        assert!(state.entries.contains_key(&a));
        assert!(state.entries.contains_key(&c));
        assert!(state.entries.contains_key(&d));
        assert_eq!(state.lru.front(), Some(&c));
        assert_eq!(state.lru.back(), Some(&d));
    }

    #[test]
    fn changed_identity_removes_entry_and_lru_key() {
        let identity = FileIdentity {
            size: 1,
            modified: 2,
            head: 3,
            tail: 4,
        };
        let key = Key {
            path: PathBuf::from("changed"),
            operation: Operation::Decode,
        };
        let image =
            Arc::new(Raster::solid(Dimensions::new(2, 2).unwrap(), [1, 2, 3, 255]).unwrap());
        let mut state = State::default();
        state.insert(key.clone(), identity, image, 48);
        let changed = FileIdentity {
            head: 5,
            ..identity
        };

        assert!(state.lookup(&key, changed).is_none());
        assert!(state.entries.is_empty());
        assert!(state.lru.is_empty());
        assert_eq!(state.bytes, 0);
    }
}
