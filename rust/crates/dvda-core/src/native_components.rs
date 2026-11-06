//! Access to the linked media component and explicit image comparison adapter.
use dvda_native::{images::Images, media::Media};
use std::{io, path::Path, sync::Arc};

pub(crate) fn media(_path: &Path) -> Arc<Media> {
    Media::linked()
}

pub(crate) fn images(path: Option<&Path>) -> io::Result<Arc<Images>> {
    Images::load(path.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Menu image component is missing")
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_media_ignores_legacy_paths_and_image_oracle_is_explicit() {
        let path = Path::new("missing-native-component.dll");
        let _media = media(path);
        assert!(images(None).is_err());
    }
}
