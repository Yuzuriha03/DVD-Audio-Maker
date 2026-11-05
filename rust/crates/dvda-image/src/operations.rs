use crate::{
    geometry::{Dimensions, ResizeMode},
    raster::Raster,
};
use std::io;

/// Typed pixel operations; menu layout and navigation remain with the caller.
pub enum Operation<'a> {
    Resize {
        size: Dimensions,
        mode: ResizeMode,
    },
    Extent {
        size: Dimensions,
        background: [u8; 4],
    },
    Rectangle {
        bounds: [i32; 4],
        color: [u8; 4],
    },
    Triangle {
        points: [[i32; 2]; 3],
        color: [u8; 4],
    },
    Composite {
        source: &'a Raster,
        x: i32,
        y: i32,
    },
    Text {
        mask: &'a crate::text::TextMask,
        x: i32,
        baseline: i32,
        color: [u8; 4],
    },
}

/// A failed or cancelled session cannot subsequently publish a partial image.
pub struct Session {
    image: Option<Raster>,
}

impl Session {
    #[must_use]
    pub fn new(image: Raster) -> Self {
        Self { image: Some(image) }
    }

    /// Apply image operations atomically within this session.
    ///
    /// # Errors
    /// Returns an error when an operation fails, allocation fails, or cancellation is requested.
    pub fn apply(
        &mut self,
        operations: &[Operation<'_>],
        cancelled: &mut dyn FnMut() -> bool,
    ) -> io::Result<()> {
        let mut image = self
            .image
            .take()
            .ok_or_else(|| io::Error::other("image session has failed"))?;
        for operation in operations {
            check_cancelled(cancelled)?;
            match operation {
                Operation::Resize { size, mode } => image = image.resized(*size, *mode)?,
                Operation::Extent { size, background } => {
                    image = image.centered_extent(*size, *background)?;
                }
                Operation::Rectangle { bounds, color } => image.fill_rectangle(*bounds, *color),
                Operation::Triangle { points, color } => image.fill_triangle(*points, *color),
                Operation::Composite { source, x, y } => image.composite(source, *x, *y),
                Operation::Text {
                    mask,
                    x,
                    baseline,
                    color,
                } => image.draw_text_mask(mask, *x, *baseline, *color)?,
            }
        }
        check_cancelled(cancelled)?;
        self.image = Some(image);
        Ok(())
    }

    /// Consume the session and return its final image.
    ///
    /// # Errors
    /// Returns an error if a previous operation left the session failed.
    pub fn finish(self) -> io::Result<Raster> {
        self.image
            .ok_or_else(|| io::Error::other("image session has failed"))
    }
}

fn check_cancelled(cancelled: &mut dyn FnMut() -> bool) -> io::Result<()> {
    if cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "image operation cancelled",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        Session::new(Raster::solid(Dimensions::new(2, 2).unwrap(), [0; 4]).unwrap())
    }
    #[test]
    fn cancellation_discards_partial_result() {
        let mut session = session();
        let mut checks = 0;
        let error = session
            .apply(
                &[Operation::Rectangle {
                    bounds: [0, 0, 1, 1],
                    color: [255; 4],
                }],
                &mut || {
                    checks += 1;
                    checks == 2
                },
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(session.finish().is_err());
    }
    #[test]
    fn failed_session_cannot_resume() {
        let mut session = session();
        let mask = crate::text::TextMask {
            advance: 0.0,
            glyphs: vec![crate::text::GlyphMask {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                coverage: vec![],
            }],
        };
        assert!(
            session
                .apply(
                    &[Operation::Text {
                        mask: &mask,
                        x: 0,
                        baseline: 0,
                        color: [255; 4]
                    }],
                    &mut || false
                )
                .is_err()
        );
        assert!(session.apply(&[], &mut || false).is_err());
        assert!(session.finish().is_err());
    }
    #[test]
    fn operations_execute_in_order() {
        let mut session = session();
        session
            .apply(
                &[
                    Operation::Rectangle {
                        bounds: [0, 0, 1, 1],
                        color: [255; 4],
                    },
                    Operation::Rectangle {
                        bounds: [1, 1, 1, 1],
                        color: [255, 0, 0, 255],
                    },
                ],
                &mut || false,
            )
            .unwrap();
        let image = session.finish().unwrap();
        assert_eq!(&image.rgba()[..4], &[255; 4]);
        assert_eq!(&image.rgba()[12..], &[255, 0, 0, 255]);
    }
}
