//! Access to the linked media component.
use dvda_native::media::Media;
use std::{path::Path, sync::Arc};

pub(crate) fn media(_path: &Path) -> Arc<Media> {
    Media::linked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_media_ignores_legacy_paths() {
        let _media = media(Path::new("missing-native-component.dll"));
    }
}
