//! The Ratty Graphics Protocol codec: 3D objects as first-class inline
//! terminal objects, per the ratty project's specification. Inside ratty
//! the terminal renders the objects; everywhere else katui scenes fall
//! back to images, so this codec costs nothing until a ratty answers.

use std::fmt::Write as _;

/// What a ratty terminal reported about its graphics support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Support {
    /// The protocol version, 1 today.
    pub version: u8,
    /// The feature flags the terminal set.
    pub features: Features,
}

/// The support flags, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Features(u8);

impl Features {
    /// No features.
    pub const NONE: Self = Self(0);
    /// Payload-based registration, the ssh-friendly form.
    pub const PAYLOAD: Self = Self(1 << 0);
    /// Chunked payload registration for large assets.
    pub const CHUNK: Self = Self(1 << 1);
    /// Camera control.
    pub const CAMERA: Self = Self(1 << 2);
    /// In-place object updates.
    pub const UPDATE: Self = Self(1 << 3);

    /// Both feature sets at once.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every feature of `other` is set here.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// The support query: `ESC _ ratty;g;s ESC \`. A ratty answers before any
/// keystroke, no answer means no support.
#[must_use]
pub fn support_query() -> Vec<u8> {
    rgp_apc("s")
}

/// Parse a support answer of the shape
/// `ratty;g;s;v=1;fmt=obj|glb|stl;path=1;payload=1;chunk=1;...`.
/// Returns `None` for anything else.
#[must_use]
pub fn parse_support(input: &[u8]) -> Option<Support> {
    let text = std::str::from_utf8(input).ok()?;
    let frame = text
        .strip_prefix("\x1b_ratty;g;s;")?
        .strip_suffix("\x1b\\")?;
    let mut support = Support::default();
    let mut features = Features::NONE;
    for pair in frame.split(';') {
        let (key, value) = pair.split_once('=')?;
        match key {
            "v" => support.version = value.parse().ok()?,
            "payload" if value == "1" => features = features.union(Features::PAYLOAD),
            "chunk" if value == "1" => features = features.union(Features::CHUNK),
            "camera" if value == "1" => features = features.union(Features::CAMERA),
            "update" if value == "1" => features = features.union(Features::UPDATE),
            _ => {}
        }
    }
    support.features = features;
    (support.version > 0).then_some(support)
}

/// Register an asset by path, one object id per model.
#[must_use]
pub fn register_path(id: u32, format: &str, path: &str) -> Vec<u8> {
    rgp_apc(&format!("r;id={id};fmt={format};path={path}"))
}

/// Place a registered object: `row`/`col` name the anchor cell at the
/// center of the placement, `w`/`h` its cell span, `ry` an optional
/// yaw in degrees.
#[must_use]
pub fn place(id: u32, row: u32, col: u32, w: u32, h: u32, ry: Option<i32>) -> Vec<u8> {
    let mut body = format!("p;id={id};row={row};col={col};w={w};h={h}");
    if let Some(ry) = ry {
        let _ = write!(body, ";ry={ry}");
    }
    rgp_apc(&body)
}

/// Update one mutable field of a placed object: its yaw in degrees.
#[must_use]
pub fn update_ry(id: u32, ry: i32) -> Vec<u8> {
    rgp_apc(&format!("u;id={id};ry={ry}"))
}

/// Delete one object by id, or every ratty graphics object.
#[must_use]
pub fn delete(id: Option<u32>) -> Vec<u8> {
    match id {
        Some(id) => rgp_apc(&format!("d;id={id}")),
        None => rgp_apc("d"),
    }
}

fn rgp_apc(body: &str) -> Vec<u8> {
    format!("\x1b_ratty;g;{body}\x1b\\").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::{
        Features, Support, delete, parse_support, place, register_path, support_query, update_ry,
    };

    #[test]
    fn the_support_query_is_one_exact_frame() {
        assert_eq!(support_query(), b"\x1b_ratty;g;s\x1b\\".to_vec());
    }

    #[test]
    fn a_full_support_answer_parses() {
        let reply = b"\x1b_ratty;g;s;v=1;fmt=obj|glb|stl;path=1;payload=1;chunk=1;anim=1;depth=1;color=1;brightness=1;transform=1;update=1;normalize=1;camera=1\x1b\\";
        assert_eq!(
            parse_support(reply),
            Some(Support {
                version: 1,
                features: Features::PAYLOAD
                    .union(Features::CHUNK)
                    .union(Features::CAMERA)
                    .union(Features::UPDATE),
            })
        );
        let parsed = parse_support(reply).expect("the reply parses");
        assert!(parsed.features.contains(Features::PAYLOAD));
        assert!(parsed.features.contains(Features::CAMERA));
    }

    #[test]
    fn anything_else_is_no_support() {
        assert_eq!(parse_support(b"\x1b_ratty;g;s;path=1\x1b\\"), None);
        assert_eq!(parse_support(b"keystroke noise"), None);
        assert_eq!(parse_support(b""), None);
    }

    #[test]
    fn the_verbs_encode_to_golden_frames() {
        assert_eq!(
            register_path(42, "obj", "rat.obj"),
            b"\x1b_ratty;g;r;id=42;fmt=obj;path=rat.obj\x1b\\".to_vec()
        );
        assert_eq!(
            place(42, 12, 8, 4, 2, Some(45)),
            b"\x1b_ratty;g;p;id=42;row=12;col=8;w=4;h=2;ry=45\x1b\\".to_vec()
        );
        assert_eq!(
            place(42, 12, 8, 4, 2, None),
            b"\x1b_ratty;g;p;id=42;row=12;col=8;w=4;h=2\x1b\\".to_vec()
        );
        assert_eq!(
            update_ry(42, 180),
            b"\x1b_ratty;g;u;id=42;ry=180\x1b\\".to_vec()
        );
        assert_eq!(delete(Some(42)), b"\x1b_ratty;g;d;id=42\x1b\\".to_vec());
        assert_eq!(delete(None), b"\x1b_ratty;g;d\x1b\\".to_vec());
    }
}
