//! Arcade games: a C game drawn into a `KatuiLight` buffer at a fixed step.

use std::ffi::{CString, c_char, c_void};
use std::sync::Arc;

use katui_bindings::game::{Frame, GameHost, held};
use katui_game::{Blend, Camera, HeldKeys, Light, P2, Rgb, Rng, Spark, Sparks, V3, v3};
use katui_proto::input::{InputEvent, Key};

use crate::element::{KatuiImage, image};
use crate::event::{KatuiEvent, key};
use crate::ffi::{
    Error, KatuiStatus, guard, owned, owned_or_null, raise, slice, status, text, texts,
};

/// The first kitty image id games use.
pub const KATUI_BASE_IMAGE: u32 = 900;
/// How much smaller games draw under a terminal multiplexer.
pub const KATUI_MULTIPLEXED_SHRINK: u32 = 2;
const _: () = assert!(KATUI_BASE_IMAGE == katui_game::BASE_IMAGE);
const _: () = assert!(KATUI_MULTIPLEXED_SHRINK == katui_game::MULTIPLEXED_SHRINK);

/// Light, in linear channels; values above 1 bloom.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KatuiRgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

fn rgb(color: KatuiRgb) -> Rgb {
    [color.r, color.g, color.b]
}

impl From<Rgb> for KatuiRgb {
    fn from([r, g, b]: Rgb) -> Self {
        Self { r, g, b }
    }
}

/// A point in the light buffer's pixels.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KatuiPoint {
    pub x: f32,
    pub y: f32,
}

fn p2(point: KatuiPoint) -> P2 {
    P2::new(point.x, point.y)
}

/// A point or direction in world space.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KatuiV3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<KatuiV3> for V3 {
    fn from(at: KatuiV3) -> Self {
        v3(at.x, at.y, at.z)
    }
}

impl From<V3> for KatuiV3 {
    fn from(at: V3) -> Self {
        Self {
            x: at.x,
            y: at.y,
            z: at.z,
        }
    }
}

/// How a shape meets the light under it.
pub type KatuiBlend = u32;
/// Cover what is there.
pub const KATUI_BLEND_OVER: KatuiBlend = 0;
/// Add to what is there.
pub const KATUI_BLEND_ADD: KatuiBlend = 1;

fn blend(blend: KatuiBlend) -> Blend {
    if blend == KATUI_BLEND_ADD {
        Blend::Add
    } else {
        Blend::Over
    }
}

/// The light that shows as the sRGB color `red`, `green`, `blue`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_srgb(red: u8, green: u8, blue: u8) -> KatuiRgb {
    katui_game::srgb(red, green, blue).into()
}

/// `color` with every channel times `factor`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_scaled(color: KatuiRgb, factor: f32) -> KatuiRgb {
    katui_game::scaled(rgb(color), factor).into()
}

/// The color `amount` of the way from `start` to `end`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_lerp3(start: KatuiRgb, end: KatuiRgb, amount: f32) -> KatuiRgb {
    katui_game::lerp3(rgb(start), rgb(end), amount).into()
}

/// A floating-point framebuffer games add light to.
pub struct KatuiLight(Light);

#[unsafe(no_mangle)]
pub extern "C" fn katui_light_new(width: u32, height: u32, pixel_aspect: f32) -> *mut KatuiLight {
    owned(KatuiLight(Light::new(width, height, pixel_aspect)))
}

/// Free a light the caller made. NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_free(light: Option<Box<KatuiLight>>) {
    drop(light);
}

fn draw(light: Option<&mut KatuiLight>, paint: impl FnOnce(&mut Light)) {
    if let Some(light) = light {
        paint(&mut light.0);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_light_width(light: Option<&KatuiLight>) -> u32 {
    light.map_or(0, |light| light.0.width())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_light_height(light: Option<&KatuiLight>) -> u32 {
    light.map_or(0, |light| light.0.height())
}

/// One pixel's width over its height.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_pixel_aspect(light: Option<&KatuiLight>) -> f32 {
    light.map_or(1.0, |light| light.0.pixel_aspect())
}

/// Copy the pixels of `other`, a light of the same size, into `light`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_copy_from(
    light: Option<&mut KatuiLight>,
    other: Option<&KatuiLight>,
) {
    let Some(other) = other else {
        return;
    };
    draw(light, |light| light.copy_from(&other.0));
}

/// The light of the pixel at `x`, `y`.
pub type KatuiShade =
    Option<unsafe extern "C" fn(user: *mut c_void, x: usize, y: usize) -> KatuiRgb>;

/// Fill every pixel with `shade(user, x, y)`, for backdrops baked once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_paint(
    light: Option<&mut KatuiLight>,
    shade: KatuiShade,
    user: *mut c_void,
) {
    let Some(shade) = shade else {
        return;
    };
    draw(light, |light| {
        // SAFETY: our caller promises that `shade` takes `user`.
        light.paint(|x, y| rgb(unsafe { shade(user, x, y) }));
    });
}

/// Fade the last frame by `factor`, 0 for black and 1 to keep it.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_dim(light: Option<&mut KatuiLight>, factor: f32) {
    draw(light, |light| light.dim(factor));
}

/// Write the light at `x`, `y` into `color`. False outside the buffer.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_pixel(
    light: Option<&KatuiLight>,
    x: u32,
    y: u32,
    color: Option<&mut KatuiRgb>,
) -> bool {
    let Some(found) = light.and_then(|light| light.0.pixel(x, y)) else {
        return false;
    };
    if let Some(color) = color {
        *color = found.into();
    }
    true
}

/// Set the light at `x`, `y`; a spot outside the buffer is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_set_pixel(
    light: Option<&mut KatuiLight>,
    x: u32,
    y: u32,
    color: KatuiRgb,
) {
    draw(light, |light| light.set_pixel(x, y, rgb(color)));
}

/// Draw `image` with its top left at `at`, blended by its alpha.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_image(
    light: Option<&mut KatuiLight>,
    at: KatuiPoint,
    image: Option<&KatuiImage>,
) {
    if let Some(image) = image {
        draw(light, |light| light.image(p2(at), &image.0));
    }
}

/// Text in the UI font, `size` pixels tall, with its top left at `at`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_ui_text(
    light: Option<&mut KatuiLight>,
    at: KatuiPoint,
    text: *const c_char,
    size: f32,
    color: KatuiRgb,
) {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    let text = unsafe { self::text(text) };
    draw(light, |light| {
        light.ui_text(p2(at), &text, size, rgb(color));
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_ui_text_width(
    light: Option<&KatuiLight>,
    text: *const c_char,
    size: f32,
) -> f32 {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    let text = unsafe { self::text(text) };
    light.map_or(0.0, |light| light.0.ui_text_width(&text, size))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_light_rect(
    light: Option<&mut KatuiLight>,
    at: KatuiPoint,
    width: f32,
    height: f32,
    color: KatuiRgb,
    mode: KatuiBlend,
) {
    draw(light, |light| {
        light.rect(p2(at), width, height, rgb(color), blend(mode));
    });
}

/// A filled polygon through `count` points.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_polygon(
    light: Option<&mut KatuiLight>,
    points: *const KatuiPoint,
    count: usize,
    color: KatuiRgb,
    mode: KatuiBlend,
) {
    // SAFETY: the header requires `points` to hold `count` values.
    let points: Vec<P2> = unsafe { slice(points, count) }
        .iter()
        .copied()
        .map(p2)
        .collect();
    draw(light, |light| {
        light.polygon(&points, rgb(color), blend(mode));
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_light_line(
    light: Option<&mut KatuiLight>,
    from: KatuiPoint,
    to: KatuiPoint,
    width: f32,
    color: KatuiRgb,
) {
    draw(light, |light| {
        light.line(p2(from), p2(to), width, rgb(color));
    });
}

/// A soft disc of light that fades out at `radius`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_glow(
    light: Option<&mut KatuiLight>,
    center: KatuiPoint,
    radius: f32,
    color: KatuiRgb,
) {
    draw(light, |light| light.glow(p2(center), radius, rgb(color)));
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_light_ring(
    light: Option<&mut KatuiLight>,
    center: KatuiPoint,
    radius: f32,
    width: f32,
    color: KatuiRgb,
) {
    draw(light, |light| {
        light.ring(p2(center), radius, width, rgb(color));
    });
}

/// Pixel-font text with its top left at `at`, each font pixel `scale`
/// pixels wide.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_text(
    light: Option<&mut KatuiLight>,
    at: KatuiPoint,
    text: *const c_char,
    scale: u32,
    color: KatuiRgb,
) {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    let text = unsafe { self::text(text) };
    draw(light, |light| light.text(p2(at), &text, scale, rgb(color)));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_blended_text(
    light: Option<&mut KatuiLight>,
    at: KatuiPoint,
    text: *const c_char,
    scale: u32,
    color: KatuiRgb,
    mode: KatuiBlend,
) {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    let text = unsafe { self::text(text) };
    draw(light, |light| {
        light.blended_text(p2(at), &text, scale, rgb(color), blend(mode));
    });
}

/// Pixel-font text centered on `center_x`, its top at `y`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_centered_text(
    light: Option<&mut KatuiLight>,
    center_x: f32,
    y: f32,
    text: *const c_char,
    scale: u32,
    color: KatuiRgb,
) {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    let text = unsafe { self::text(text) };
    draw(light, |light| {
        light.centered_text(center_x, y, &text, scale, rgb(color));
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_light_text_width(
    light: Option<&KatuiLight>,
    text: *const c_char,
    scale: u32,
) -> f32 {
    // SAFETY: the header requires `text` to be NULL or NUL-terminated.
    let text = unsafe { self::text(text) };
    light.map_or(0.0, |light| light.0.text_width(&text, scale))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_light_text_height(scale: u32) -> f32 {
    Light::text_height(scale)
}

/// Spread light above `threshold` into a glow of `strength`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_bloom(light: Option<&mut KatuiLight>, threshold: f32, strength: f32) {
    draw(light, |light| light.bloom(threshold, strength));
}

/// The buffer tone-mapped into an image the caller owns.
#[unsafe(no_mangle)]
pub extern "C" fn katui_light_to_image(light: Option<&KatuiLight>) -> *mut KatuiImage {
    light.map_or(std::ptr::null_mut(), |light| image(light.0.to_surface()))
}

/// The keys held down right now.
pub struct KatuiKeys(HeldKeys);

/// Whether the key named `name`, like `"left"` or `"a"`, is held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_keys_is_held(keys: Option<&KatuiKeys>, name: *const c_char) -> bool {
    // SAFETY: the header requires `name` to be NULL or NUL-terminated.
    let Ok(key) = key(&unsafe { text(name) }) else {
        return false;
    };
    keys.is_some_and(|keys| keys.0.is_held(key))
}

/// Whether any of the `count` keys at `names` is held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_keys_any_held(
    keys: Option<&KatuiKeys>,
    names: *const *const c_char,
    count: usize,
) -> bool {
    // SAFETY: the header requires `names` to hold `count` strings, each NULL or NUL-terminated.
    let wanted: Vec<Key> = unsafe { texts(names, count) }
        .iter()
        .filter_map(|name| key(name).ok())
        .collect();
    keys.is_some_and(|keys| keys.0.any_held(&wanted))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3(x: f32, y: f32, z: f32) -> KatuiV3 {
    KatuiV3 { x, y, z }
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_add(a: KatuiV3, b: KatuiV3) -> KatuiV3 {
    (V3::from(a) + V3::from(b)).into()
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_sub(a: KatuiV3, b: KatuiV3) -> KatuiV3 {
    (V3::from(a) - V3::from(b)).into()
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_mul(a: KatuiV3, scale: f32) -> KatuiV3 {
    (V3::from(a) * scale).into()
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_dot(a: KatuiV3, b: KatuiV3) -> f32 {
    V3::from(a).dot(b.into())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_cross(a: KatuiV3, b: KatuiV3) -> KatuiV3 {
    V3::from(a).cross(b.into()).into()
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_normalized(a: KatuiV3) -> KatuiV3 {
    V3::from(a).normalized().into()
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_rotated_x(a: KatuiV3, angle: f32) -> KatuiV3 {
    V3::from(a).rotated_x(angle).into()
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_rotated_y(a: KatuiV3, angle: f32) -> KatuiV3 {
    V3::from(a).rotated_y(angle).into()
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_v3_rotated_z(a: KatuiV3, angle: f32) -> KatuiV3 {
    V3::from(a).rotated_z(angle).into()
}

/// A pinhole camera looking down +z, with y up.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiCamera {
    /// Where the optical axis meets the screen.
    pub center: KatuiPoint,
    /// Pixels of screen height per unit of world height at unit depth.
    pub focal: f32,
    /// How far the eye sits behind z = 0.
    pub distance: f32,
    /// One pixel's width over its height.
    pub pixel_aspect: f32,
}

/// Write where `at` lands on screen into `on_screen`, and how many pixels
/// one world unit spans there into `scale`. False behind the eye.
#[unsafe(no_mangle)]
pub extern "C" fn katui_camera_project(
    camera: Option<&KatuiCamera>,
    at: KatuiV3,
    on_screen: Option<&mut KatuiPoint>,
    scale: Option<&mut f32>,
) -> bool {
    let Some(camera) = camera else {
        return false;
    };
    let camera = Camera {
        center: p2(camera.center),
        focal: camera.focal,
        distance: camera.distance,
        pixel_aspect: camera.pixel_aspect,
    };
    let Some((point, size)) = camera.project(at.into()) else {
        return false;
    };
    if let Some(on_screen) = on_screen {
        *on_screen = KatuiPoint {
            x: point.x,
            y: point.y,
        };
    }
    if let Some(scale) = scale {
        *scale = size;
    }
    true
}

/// A small deterministic generator, so games replay the same in tests.
pub struct KatuiRng(Rng);

#[unsafe(no_mangle)]
pub extern "C" fn katui_rng_new(seed: u32) -> *mut KatuiRng {
    owned(KatuiRng(Rng::new(seed)))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_rng_free(rng: Option<Box<KatuiRng>>) {
    drop(rng);
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_rng_next_u32(rng: Option<&mut KatuiRng>) -> u32 {
    rng.map_or(0, |rng| rng.0.next_u32())
}

/// A value from `low` up to `high`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_rng_range(rng: Option<&mut KatuiRng>, low: f32, high: f32) -> f32 {
    rng.map_or(low, |rng| rng.0.range(low, high))
}

/// A random direction of unit length.
#[unsafe(no_mangle)]
pub extern "C" fn katui_rng_direction(rng: Option<&mut KatuiRng>) -> KatuiV3 {
    rng.map_or_else(KatuiV3::default, |rng| rng.0.direction().into())
}

/// One glowing particle.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiSpark {
    pub at: KatuiV3,
    /// In world units per second.
    pub velocity: KatuiV3,
    pub color: KatuiRgb,
    /// Seconds lived.
    pub age: f32,
    /// Seconds lived when it dies.
    pub life: f32,
    /// Line width in pixels.
    pub width: f32,
}

/// Every live spark, aged, moved and drawn together.
pub struct KatuiSparks(Sparks);

#[unsafe(no_mangle)]
pub extern "C" fn katui_sparks_new() -> *mut KatuiSparks {
    owned(KatuiSparks(Sparks::default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_sparks_free(sparks: Option<Box<KatuiSparks>>) {
    drop(sparks);
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_sparks_add(sparks: Option<&mut KatuiSparks>, spark: KatuiSpark) {
    if let Some(sparks) = sparks {
        sparks.0.add(Spark {
            at: spark.at.into(),
            velocity: spark.velocity.into(),
            color: rgb(spark.color),
            age: spark.age,
            life: spark.life,
            width: spark.width,
        });
    }
}

/// Age and move every spark by `dt` seconds under `gravity`.
#[unsafe(no_mangle)]
pub extern "C" fn katui_sparks_step(sparks: Option<&mut KatuiSparks>, dt: f32, gravity: KatuiV3) {
    if let Some(sparks) = sparks {
        sparks.0.step(dt, gravity.into());
    }
}

/// Write where the spark at `at` shows on screen into `on_screen`; false
/// skips it.
pub type KatuiPlace = Option<
    unsafe extern "C" fn(user: *mut c_void, at: KatuiV3, on_screen: *mut KatuiPoint) -> bool,
>;

/// Draw every spark as a fading streak, placed by `place(user, at, out)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_sparks_draw(
    sparks: Option<&KatuiSparks>,
    light: Option<&mut KatuiLight>,
    place: KatuiPlace,
    user: *mut c_void,
) {
    let (Some(sparks), Some(place)) = (sparks, place) else {
        return;
    };
    draw(light, |light| {
        sparks.0.draw(light, |at| {
            let mut spot = KatuiPoint::default();
            // SAFETY: our caller promises that `place` takes `user`.
            let placed = unsafe { place(user, at.into(), &raw mut spot) };
            placed.then(|| p2(spot))
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_sparks_len(sparks: Option<&KatuiSparks>) -> usize {
    sparks.map_or(0, |sparks| sparks.0.len())
}

#[unsafe(no_mangle)]
pub extern "C" fn katui_sparks_is_empty(sparks: Option<&KatuiSparks>) -> bool {
    sparks.is_none_or(|sparks| sparks.0.is_empty())
}

/// A game as function pointers over your state at `user`. `step` and
/// `render` are required; leave the others NULL. Key names are strings
/// like `"a"`, `" "`, `"enter"` and `"left"`, owned by katui and valid
/// during the call; so are `keys` and `light`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KatuiGame {
    /// Handed to every callback as is.
    pub user: *mut c_void,
    /// `key` went down.
    pub press: Option<unsafe extern "C" fn(user: *mut c_void, key: *const c_char)>,
    /// `key` auto-repeated while held.
    pub repeat: Option<unsafe extern "C" fn(user: *mut c_void, key: *const c_char)>,
    /// The screen became the size of `light`, so baked art needs redoing.
    pub resize: Option<unsafe extern "C" fn(user: *mut c_void, light: *const KatuiLight)>,
    /// Advance the world by `dt` seconds, a fixed step.
    pub step: Option<unsafe extern "C" fn(user: *mut c_void, keys: *const KatuiKeys, dt: f32)>,
    /// Draw the world into `light`.
    pub render: Option<unsafe extern "C" fn(user: *mut c_void, light: *mut KatuiLight)>,
    /// Whether the game is over, so `katui_play` ends after this frame.
    pub finished: Option<unsafe extern "C" fn(user: *mut c_void) -> bool>,
    /// The mouse did `event` at `at`, in the light buffer's pixels: a
    /// press, release, drag, move or wheel turn.
    pub mouse:
        Option<unsafe extern "C" fn(user: *mut c_void, event: *const KatuiEvent, at: KatuiPoint)>,
    /// Whether the next frame differs from the last one drawn; NULL means
    /// always. While it returns false, `katui_play` draws nothing and steps
    /// ten times a second.
    pub changed: Option<unsafe extern "C" fn(user: *mut c_void) -> bool>,
}

/// A C game, as the shared bridge calls it.
struct CGame {
    game: KatuiGame,
    light: KatuiLight,
}

impl CGame {
    /// # Safety
    /// Every callback of `game` takes its `user`, which outlives the result.
    unsafe fn new(game: &KatuiGame) -> Result<Self, Error> {
        if game.step.is_none() {
            return Err(Error::Null("step callback"));
        }
        if game.render.is_none() {
            return Err(Error::Null("render callback"));
        }
        Ok(Self {
            game: *game,
            light: KatuiLight(Light::new(1, 1, 1.0)),
        })
    }

    fn key(&self, call: Option<unsafe extern "C" fn(*mut c_void, *const c_char)>, key: Key) {
        let (Some(call), Ok(name)) = (call, CString::new(key.name())) else {
            return;
        };
        // SAFETY: `CGame::new` requires `call` to take `user`, which outlives us.
        unsafe { call(self.game.user, name.as_ptr()) };
    }

    /// What `call` answers, or `None` when the game left it NULL.
    fn ask(&self, call: Option<unsafe extern "C" fn(*mut c_void) -> bool>) -> Option<bool> {
        // SAFETY: `CGame::new` requires `call` to take `user`, which outlives us.
        call.map(|call| unsafe { call(self.game.user) })
    }

    /// Lend `light` to `call` through our own `KatuiLight`, then take it back.
    fn with_light(&mut self, light: &mut Light, call: impl FnOnce(*mut c_void, *mut KatuiLight)) {
        std::mem::swap(&mut self.light.0, light);
        call(self.game.user, &raw mut self.light);
        std::mem::swap(&mut self.light.0, light);
    }
}

impl GameHost for CGame {
    type Error = Error;

    fn refused(error: katui_bindings::Error) -> Error {
        raise(error)
    }

    fn press(&mut self, key: Key) -> Result<(), Error> {
        self.key(self.game.press, key);
        Ok(())
    }

    fn repeat(&mut self, key: Key) -> Result<(), Error> {
        self.key(self.game.repeat, key);
        Ok(())
    }

    fn mouse(&mut self, event: &InputEvent, at: P2) -> Result<(), Error> {
        if let Some(mouse) = self.game.mouse {
            let event = KatuiEvent(event.clone());
            let at = KatuiPoint { x: at.x, y: at.y };
            // SAFETY: `CGame::new` requires `mouse` to take `user`, which outlives us.
            unsafe { mouse(self.game.user, &raw const event, at) };
        }
        Ok(())
    }

    fn resize(&mut self, light: &mut Light) -> Result<(), Error> {
        if let Some(resize) = self.game.resize {
            // SAFETY: `CGame::new` requires `resize` to take `user`, which outlives us.
            self.with_light(light, |user, light| unsafe { resize(user, light) });
        }
        Ok(())
    }

    fn step(&mut self, keys: &HeldKeys, dt: f32) -> Result<(), Error> {
        if let Some(step) = self.game.step {
            let keys = KatuiKeys(keys.clone());
            // SAFETY: `CGame::new` requires `step` to take `user`, which outlives us.
            unsafe { step(self.game.user, &raw const keys, dt) };
        }
        Ok(())
    }

    fn render(&mut self, light: &mut Light) -> Result<(), Error> {
        if let Some(render) = self.game.render {
            // SAFETY: `CGame::new` requires `render` to take `user`, which outlives us.
            self.with_light(light, |user, light| unsafe { render(user, light) });
        }
        Ok(())
    }

    fn changed(&self) -> Result<bool, Error> {
        Ok(self.ask(self.game.changed).unwrap_or(true))
    }

    fn finished(&self) -> Result<bool, Error> {
        Ok(self.ask(self.game.finished).unwrap_or(false))
    }
}

/// Run `game` on the terminal until the player quits or it finishes.
/// `game` is copied; `user` has to live until this returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_play(game: Option<&KatuiGame>) -> KatuiStatus {
    let played = guard(|| {
        let game = game.ok_or(Error::Null("game"))?;
        // SAFETY: the header requires `user` to live until `katui_play` returns.
        katui_bindings::game::play(unsafe { CGame::new(game) }?)
    });
    status(played)
}

/// `game` drawn off-terminal at `width` by `height` after `steps` steps of
/// `dt` seconds, with the `held_count` keys named at `held` held down, for
/// tests and screenshots. The caller owns the image; NULL on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katui_frame(
    game: Option<&KatuiGame>,
    width: u32,
    height: u32,
    steps: u32,
    held: *const *const c_char,
    held_count: usize,
    dt: f32,
) -> *mut KatuiImage {
    // SAFETY: the header requires `held` to hold `held_count` strings, each NULL or NUL-terminated.
    let held = unsafe { texts(held, held_count) };
    let drawn = guard(|| {
        let game = game.ok_or(Error::Null("game"))?;
        // SAFETY: the header requires `user` to live until `katui_frame` returns.
        let host = unsafe { CGame::new(game) }?;
        let frame = Frame {
            width,
            height,
            steps,
            held: self::held(&held).map_err(raise)?,
            dt,
        };
        let surface = katui_bindings::game::frame(host, &frame)?;
        Ok(KatuiImage(Arc::new(surface)))
    });
    owned_or_null(drawn)
}
