//! Task-scoped decoded and processed image sharing with bounded memory.
use crate::{
    geometry::{Dimensions, ResizeMode},
    raster::{MAX_WORKING_BYTES, Raster},
};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
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
    let mut head = vec![0; 4096usize.min(metadata.len() as usize)];
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Operation {
    Decode,
    Resize {
        width: u32,
        height: u32,
        mode: u8,
        centered: bool,
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
    bytes: usize,
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
        let mut state = self.lock();
        let entry = state.entries.get(key)?;
        if entry.identity != identity {
            let entry = state.entries.remove(key).unwrap();
            state.bytes = state.bytes.saturating_sub(entry.image.rgba().len());
            return None;
        }
        Some(Arc::clone(&entry.image))
    }

    fn insert(&self, key: Key, identity: FileIdentity, image: Arc<Raster>) -> Arc<Raster> {
        let bytes = image.rgba().len();
        if bytes > MAX_WORKING_BYTES {
            return image;
        }
        let mut state = self.lock();
        if let Some(old) = state.entries.remove(&key) {
            state.bytes = state.bytes.saturating_sub(old.image.rgba().len());
        }
        if state.bytes.saturating_add(bytes) > MAX_WORKING_BYTES {
            state.entries.clear();
            state.bytes = 0;
        }
        state.bytes = state.bytes.saturating_add(bytes);
        state.entries.insert(
            key,
            Entry {
                identity,
                image: Arc::clone(&image),
            },
        );
        image
    }

    pub fn decode(&self, path: &Path) -> std::io::Result<Arc<Raster>> {
        let identity = file_identity(path)?;
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

    pub fn resized(
        &self,
        path: &Path,
        size: Dimensions,
        mode: ResizeMode,
        centered: bool,
    ) -> std::io::Result<Arc<Raster>> {
        let identity = file_identity(path)?;
        let mode = match mode {
            ResizeMode::Fit => 0,
            ResizeMode::Fill => 1,
            ResizeMode::Exact => 2,
        };
        let key = Key {
            path: path.to_path_buf(),
            operation: Operation::Resize {
                width: size.width(),
                height: size.height(),
                mode,
                centered,
            },
        };
        if let Some(image) = self.lookup(&key, identity) {
            return Ok(image);
        }
        let source = self.decode(path)?;
        let mut image = source.resized(
            size,
            match mode {
                0 => ResizeMode::Fit,
                1 => ResizeMode::Fill,
                _ => ResizeMode::Exact,
            },
        )?;
        if centered {
            image = image.centered_extent(size, [0, 0, 0, 255])?;
        }
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
        std::fs::remove_dir_all(root).unwrap();
    }
}
