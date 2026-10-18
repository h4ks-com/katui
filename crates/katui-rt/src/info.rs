//! The capability card `katui-info` prints: one line per fact, the
//! graphics backend first.

use katui_proto::probe::{Capabilities, ProbeSource};

/// Render the card for `caps`.
#[must_use]
pub fn card(caps: &Capabilities) -> String {
    let graphics = if caps.kitty {
        "kitty"
    } else if caps.sixel {
        "sixel"
    } else {
        "blocks"
    };
    let identity = match &caps.identity {
        Some(identity) => match &identity.version {
            Some(version) => format!("{} {}", identity.name, version),
            None => identity.name.clone(),
        },
        None => "unknown".to_owned(),
    };
    let cell = match caps.cell {
        Some(cell) => format!("{}x{} px", cell.width, cell.height),
        None => "unknown".to_owned(),
    };
    let source = match caps.trust {
        ProbeSource::Probed => "probed",
        ProbeSource::Sniffed => "sniffed",
        ProbeSource::Declared => "declared",
    };
    format!(
        "graphics: {graphics}\nidentity: {identity}\ntruecolor: {}\ncell: {cell}\nsource: {source}\n",
        if caps.truecolor { "yes" } else { "no" }
    )
}

#[cfg(test)]
mod tests {
    use super::card;
    use katui_proto::probe::{Capabilities, CellPixels, Identity, ProbeSource};

    #[test]
    fn a_probed_kitty_session_prints_every_fact() {
        let caps = Capabilities {
            kitty: true,
            sixel: false,
            truecolor: true,
            identity: Some(Identity {
                name: "kitty".to_owned(),
                version: Some("0.29.0".to_owned()),
            }),
            cell: Some(CellPixels {
                width: 7,
                height: 14,
            }),
            trust: ProbeSource::Probed,
        };
        assert_eq!(
            card(&caps),
            "graphics: kitty\nidentity: kitty 0.29.0\ntruecolor: yes\ncell: 7x14 px\nsource: probed\n"
        );
    }

    #[test]
    fn a_silent_terminal_falls_back_to_the_floor() {
        let caps = Capabilities {
            trust: ProbeSource::Sniffed,
            ..Capabilities::default()
        };
        assert_eq!(
            card(&caps),
            "graphics: blocks\nidentity: unknown\ntruecolor: no\ncell: unknown\nsource: sniffed\n"
        );
    }
}
