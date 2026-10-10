//! Feature-aware access to the bundled media and image components.
use dvda_native::{images::Images, media::Media};
use std::{io, path::Path, sync::Arc};

pub(crate) fn media_available(path: &Path) -> bool {
    cfg!(feature = "direct-bridges") || path.is_file()
}

pub(crate) fn images_available(path: Option<&Path>) -> bool {
    cfg!(feature = "direct-bridges") || path.is_some_and(Path::is_file)
}

pub(crate) fn media(path: &Path) -> io::Result<Arc<Media>> {
    #[cfg(feature = "direct-bridges")]
    {
        let _ = path;
        Ok(Media::linked())
    }
    #[cfg(not(feature = "direct-bridges"))]
    Media::load(path)
}

pub(crate) fn images(path: Option<&Path>) -> io::Result<Arc<Images>> {
    #[cfg(feature = "direct-bridges")]
    {
        let _ = path;
        Ok(Images::linked())
    }
    #[cfg(not(feature = "direct-bridges"))]
    Images::load(path.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Menu image component is missing")
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_paths_follow_selected_backend() {
        let path = Path::new("missing-native-component.dll");
        assert_eq!(media_available(path), cfg!(feature = "direct-bridges"));
        assert_eq!(images_available(None), cfg!(feature = "direct-bridges"));
        #[cfg(feature = "direct-bridges")]
        {
            assert!(media(path).is_ok());
            assert!(images(None).is_ok());
        }
        #[cfg(not(feature = "direct-bridges"))]
        assert!(images(None).is_err());
    }
}
