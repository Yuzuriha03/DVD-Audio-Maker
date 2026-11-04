#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VideoFormat {
    Pal,
    Ntsc,
}

impl VideoFormat {
    pub(crate) fn frame_duration(self) -> u32 {
        match self {
            Self::Pal => 3_600,
            Self::Ntsc => 3_003,
        }
    }
}
