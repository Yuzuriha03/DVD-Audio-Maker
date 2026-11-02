use dvda_image::{
    fonts::{FontCache, FontLocation},
    text::shape_masks,
};
use std::{env, io, path::PathBuf, sync::Arc};

fn main() -> io::Result<()> {
    let path = env::args_os().nth(1).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "usage: font-probe <font.ttc>")
    })?;
    let mut cache = FontCache::default();
    let location = FontLocation {
        path: PathBuf::from(path),
        face_index: 0,
    };
    let data = cache.load(&location)?;
    assert!(Arc::ptr_eq(&data, &cache.load(&location)?));
    for face_index in 1..4 {
        assert!(Arc::ptr_eq(
            &data,
            &cache.load(&FontLocation {
                path: location.path.clone(),
                face_index
            })?
        ));
    }
    assert!(
        cache
            .load(&FontLocation {
                path: location.path.clone(),
                face_index: u32::MAX
            })
            .is_err()
    );
    for label in [
        "Previous Next",
        "中文菜单",
        "日本語メニュー",
        "한국어 메뉴",
        "% @ \\",
    ] {
        for antialias in [false, true] {
            let mask = shape_masks(&data, 0, label, 20.0, antialias)?;
            assert!(mask.advance > 0.0);
            assert!(!mask.glyphs.is_empty());
            assert!(
                mask.glyphs
                    .iter()
                    .any(|g| g.coverage.iter().any(|v| *v != 0))
            );
            for glyph in &mask.glyphs {
                assert_eq!(
                    glyph.coverage.len(),
                    glyph.width as usize * glyph.height as usize
                );
                if !antialias {
                    assert!(glyph.coverage.iter().all(|value| matches!(value, 0 | 255)));
                }
            }
            println!(
                "{label:?}: antialias={antialias}, advance={}, glyphs={}",
                mask.advance,
                mask.glyphs.len()
            );
        }
    }
    Ok(())
}
