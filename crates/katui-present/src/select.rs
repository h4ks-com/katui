//! Backend selection: capabilities in, graphics backend out.

use katui_proto::probe::Capabilities;

/// The graphics backend a presenter strategy drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The kitty graphics protocol: images, placements, animation.
    Kitty,
    /// Sixel: paletted bands, one shot per image.
    Sixel,
    /// Half and quarter block glyphs over SGR truecolor, the floor every
    /// color terminal understands.
    Blocks,
}

/// Pick the best backend the terminal offers. Kitty wins when present,
/// sixel comes second, blocks carry everything else. Needs-aware choices
/// (alpha, z-order, frame budget) join when the presenter strategies do.
pub fn select(caps: &Capabilities) -> Backend {
    if caps.kitty {
        Backend::Kitty
    } else if caps.sixel {
        Backend::Sixel
    } else {
        Backend::Blocks
    }
}

#[cfg(test)]
mod tests {
    use super::{Backend, select};
    use katui_proto::probe::{Capabilities, ProbeSource};

    fn caps(kitty: bool, sixel: bool) -> Capabilities {
        Capabilities {
            kitty,
            sixel,
            truecolor: true,
            identity: None,
            cell: None,
            trust: ProbeSource::Probed,
        }
    }

    #[test]
    fn kitty_outranks_everything() {
        assert_eq!(select(&caps(true, true)), Backend::Kitty);
        assert_eq!(select(&caps(true, false)), Backend::Kitty);
    }

    #[test]
    fn sixel_comes_second() {
        assert_eq!(select(&caps(false, true)), Backend::Sixel);
    }

    #[test]
    fn blocks_carry_the_rest() {
        assert_eq!(select(&caps(false, false)), Backend::Blocks);
    }
}
