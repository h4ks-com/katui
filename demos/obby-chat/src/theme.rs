//! The rainbow obsidian palette; each nick takes a fixed hue from it.

use katui_draw::{Paint, Rgba};

pub const ASH: Rgba = Rgba::solid(207, 200, 217);
pub const PUMICE: Rgba = Rgba::solid(139, 131, 152);
pub const SULFUR: Rgba = Rgba::solid(242, 212, 92);
pub const INK_ON_LIGHT: Rgba = Rgba::solid(27, 24, 38);
pub const AVATAR: Rgba = Rgba::solid(12, 10, 16);
pub const ONLINE: Rgba = Rgba::solid(67, 181, 129);
pub const QUOTE: Rgba = Rgba {
    r: 18,
    g: 16,
    b: 26,
    a: 190,
};
pub const WHITE: Rgba = Rgba::solid(255, 255, 255);
pub const MENU: Rgba = Rgba::solid(52, 45, 68);
pub const DIM_DOT: Rgba = Rgba::solid(74, 66, 92);

const fn glass(alpha: u8) -> Rgba {
    Rgba {
        r: 42,
        g: 36,
        b: 56,
        a: alpha,
    }
}

pub fn panel() -> Paint {
    Paint::Solid(glass(214))
}

pub fn rail() -> Paint {
    Paint::Solid(Rgba {
        r: 27,
        g: 24,
        b: 38,
        a: 232,
    })
}

pub fn scrim() -> Paint {
    Paint::Solid(Rgba {
        r: 12,
        g: 10,
        b: 18,
        a: 150,
    })
}

pub fn raised() -> Paint {
    Paint::Solid(Rgba {
        r: 62,
        g: 54,
        b: 80,
        a: 235,
    })
}

pub fn lifted() -> Paint {
    Paint::Solid(Rgba {
        r: 207,
        g: 200,
        b: 217,
        a: 20,
    })
}

/// A color in floating-point channels, for mixing.
#[derive(Clone, Copy)]
pub struct Tint {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

/// A color at `hue` in turns.
pub fn hue_tint(hue: f32, saturation: f32, lightness: f32) -> Tint {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(1.0) * 6.0;
    let middle = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, middle, 0.0),
        1 => (middle, chroma, 0.0),
        2 => (0.0, chroma, middle),
        3 => (0.0, middle, chroma),
        4 => (middle, 0.0, chroma),
        _ => (chroma, 0.0, middle),
    };
    let offset = lightness - chroma / 2.0;
    Tint {
        red: (red + offset) * 255.0,
        green: (green + offset) * 255.0,
        blue: (blue + offset) * 255.0,
    }
}

/// The stable hue of a nick, in turns.
pub fn nick_hue(nick: &str) -> f32 {
    let hash = nick.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    (hash % 360) as f32 / 360.0
}

pub fn nick_color(nick: &str) -> Rgba {
    let tint = hue_tint(nick_hue(nick), 0.72, 0.72);
    Rgba::solid(tint.red as u8, tint.green as u8, tint.blue as u8)
}
