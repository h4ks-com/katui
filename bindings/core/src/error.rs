//! The failures every binding reports in its own style.

/// A wrong argument, a failed read or write, or a runner that stopped.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("name a key, not {0:?}")]
    Key(String),
    #[error("align to \"start\", \"center\" or \"end\", not {0:?}")]
    Align(String),
    #[error("give a length in pixels, \"fill\" or \"content\", not {0:?}")]
    Length(String),
    #[error("split \"horizontal\" or \"vertical\", not {0:?}")]
    Orientation(String),
    #[error("blend \"over\" or \"add\", not {0:?}")]
    Blend(String),
    #[error("write the color {0:?} as \"#rrggbb\"")]
    Color(String),
    #[error("give exactly {width} x {height} x 4 bytes of RGBA")]
    ImageSize { width: u32, height: u32 },
    #[error("decode the image: {0}")]
    Decode(image::ImageError),
    #[error("read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("write {path}: {source}")]
    Write {
        path: String,
        source: image::ImageError,
    },
    #[error("load the fonts: {0}")]
    Fonts(String),
    #[error("draw a screen of at least one pixel")]
    EmptyScreen,
    #[error("{0}")]
    Terminal(String),
}

/// Which family of exception an [`Error`] maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The caller passed a value katui cannot use.
    Argument,
    /// A file or the terminal failed.
    Io,
    /// katui could not do what was asked.
    Runtime,
}

impl Error {
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Key(_)
            | Self::Align(_)
            | Self::Length(_)
            | Self::Orientation(_)
            | Self::Blend(_)
            | Self::Color(_)
            | Self::ImageSize { .. }
            | Self::Decode(_) => ErrorKind::Argument,
            Self::Read { .. } | Self::Write { .. } | Self::Terminal(_) => ErrorKind::Io,
            Self::Fonts(_) | Self::EmptyScreen => ErrorKind::Runtime,
        }
    }
}
