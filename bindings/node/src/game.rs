//! Arcade games: a JavaScript game drawn into a `Light` buffer at a fixed step.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use katui_game::{
    Blend, Camera as UiCamera, Game as UiGame, HeldKeys, Light as UiLight, P2, Rgb, Rng as UiRng,
    Spark as UiSpark, Sparks as UiSparks, V3 as UiV3, v3,
};
use katui_proto::input::Key;
use napi::bindgen_prelude::{ClassInstance, Either, FnArgs, Function, JavaScriptClassExt, Object};
use napi::{Env, JsValue, Result};

use crate::app::{Failure, call_optional, keep, object, take, terminal_error};
use crate::convert::invalid;
use crate::element::Image;
use crate::message::{Obj, fetch, hold};
use crate::state::{key, key_name};

type Color = (f64, f64, f64);
type Point = (f64, f64);

fn rgb((r, g, b): Color) -> Rgb {
    [r as f32, g as f32, b as f32]
}

fn color_tuple([r, g, b]: Rgb) -> Color {
    (f64::from(r), f64::from(g), f64::from(b))
}

fn p2((x, y): Point) -> P2 {
    P2::new(x as f32, y as f32)
}

fn blend(name: Option<&str>, default: Blend) -> Result<Blend> {
    match name {
        None => Ok(default),
        Some("over") => Ok(Blend::Over),
        Some("add") => Ok(Blend::Add),
        Some(other) => Err(invalid(format!("blend \"over\" or \"add\", not {other:?}"))),
    }
}

/// The light that shows as the sRGB color `red`, `green`, `blue`.
#[napi(ts_return_type = "LightColor")]
pub fn srgb(red: u8, green: u8, blue: u8) -> Color {
    color_tuple(katui_game::srgb(red, green, blue))
}

/// `color` with every channel times `factor`.
#[napi(
    ts_args_type = "color: LightColor, factor: number",
    ts_return_type = "LightColor"
)]
pub fn scaled(color: Color, factor: f64) -> Color {
    color_tuple(katui_game::scaled(rgb(color), factor as f32))
}

/// The color `amount` of the way from `start` to `end`.
#[napi(
    ts_args_type = "start: LightColor, end: LightColor, amount: number",
    ts_return_type = "LightColor"
)]
pub fn lerp3(start: Color, end: Color, amount: f64) -> Color {
    color_tuple(katui_game::lerp3(rgb(start), rgb(end), amount as f32))
}

/// The first kitty image id games use.
#[napi]
pub const BASE_IMAGE: u32 = katui_game::BASE_IMAGE;

/// How much smaller games draw under a terminal multiplexer.
#[napi]
pub const MULTIPLEXED_SHRINK: u32 = katui_game::MULTIPLEXED_SHRINK;

/// A floating-point framebuffer games add light to.
#[napi]
pub struct Light {
    inner: UiLight,
}

#[napi]
impl Light {
    #[napi(constructor)]
    pub fn new(width: u32, height: u32, pixel_aspect: Option<f64>) -> Self {
        Self {
            inner: UiLight::new(width, height, pixel_aspect.unwrap_or(1.0) as f32),
        }
    }

    #[napi(getter)]
    pub fn width(&self) -> u32 {
        self.inner.width()
    }

    #[napi(getter)]
    pub fn height(&self) -> u32 {
        self.inner.height()
    }

    #[napi(getter)]
    pub fn pixel_aspect(&self) -> f64 {
        f64::from(self.inner.pixel_aspect())
    }

    #[napi]
    pub fn copy_from(&mut self, other: &Light) {
        self.inner.copy_from(&other.inner);
    }

    /// Fill every pixel with `shade(x, y)`, for backdrops baked once.
    #[napi(ts_args_type = "shade: (x: number, y: number) => LightColor")]
    pub fn paint(&mut self, shade: Function<'_, FnArgs<(u32, u32)>, Color>) -> Result<()> {
        let failed = RefCell::new(None);
        self.inner.paint(|x, y| {
            let at = (
                u32::try_from(x).unwrap_or(u32::MAX),
                u32::try_from(y).unwrap_or(u32::MAX),
            );
            match shade.call(at.into()) {
                Ok(color) => rgb(color),
                Err(error) => {
                    failed.borrow_mut().get_or_insert(error);
                    [0.0; 3]
                }
            }
        });
        failed.into_inner().map_or(Ok(()), Err)
    }

    #[napi]
    pub fn dim(&mut self, factor: f64) {
        self.inner.dim(factor as f32);
    }

    #[napi(
        ts_args_type = "at: Point, width: number, height: number, color: LightColor, blend?: Blend"
    )]
    pub fn rect(
        &mut self,
        at: Point,
        width: f64,
        height: f64,
        color: Color,
        blend: Option<String>,
    ) -> Result<()> {
        let blend = self::blend(blend.as_deref(), Blend::Over)?;
        self.inner
            .rect(p2(at), width as f32, height as f32, rgb(color), blend);
        Ok(())
    }

    #[napi(ts_args_type = "points: Point[], color: LightColor, blend?: Blend")]
    pub fn polygon(
        &mut self,
        points: Vec<Point>,
        color: Color,
        blend: Option<String>,
    ) -> Result<()> {
        let points: Vec<P2> = points.into_iter().map(p2).collect();
        let blend = self::blend(blend.as_deref(), Blend::Over)?;
        self.inner.polygon(&points, rgb(color), blend);
        Ok(())
    }

    #[napi(ts_args_type = "start: Point, end: Point, width: number, color: LightColor")]
    pub fn line(&mut self, start: Point, end: Point, width: f64, color: Color) {
        self.inner
            .line(p2(start), p2(end), width as f32, rgb(color));
    }

    #[napi(ts_args_type = "center: Point, radius: number, color: LightColor")]
    pub fn glow(&mut self, center: Point, radius: f64, color: Color) {
        self.inner.glow(p2(center), radius as f32, rgb(color));
    }

    #[napi(ts_args_type = "center: Point, radius: number, width: number, color: LightColor")]
    pub fn ring(&mut self, center: Point, radius: f64, width: f64, color: Color) {
        self.inner
            .ring(p2(center), radius as f32, width as f32, rgb(color));
    }

    #[napi(ts_args_type = "at: Point, text: string, color: LightColor, scale?: number")]
    pub fn text(&mut self, at: Point, text: String, color: Color, scale: Option<u32>) {
        self.inner
            .text(p2(at), &text, scale.unwrap_or(1), rgb(color));
    }

    #[napi(
        ts_args_type = "at: Point, text: string, color: LightColor, scale?: number, blend?: Blend"
    )]
    pub fn blended_text(
        &mut self,
        at: Point,
        text: String,
        color: Color,
        scale: Option<u32>,
        blend: Option<String>,
    ) -> Result<()> {
        let blend = self::blend(blend.as_deref(), Blend::Add)?;
        self.inner
            .blended_text(p2(at), &text, scale.unwrap_or(1), rgb(color), blend);
        Ok(())
    }

    #[napi(
        ts_args_type = "centerX: number, y: number, text: string, color: LightColor, scale?: number"
    )]
    pub fn centered_text(
        &mut self,
        center_x: f64,
        y: f64,
        text: String,
        color: Color,
        scale: Option<u32>,
    ) {
        self.inner.centered_text(
            center_x as f32,
            y as f32,
            &text,
            scale.unwrap_or(1),
            rgb(color),
        );
    }

    #[napi]
    pub fn text_width(&self, text: String, scale: Option<u32>) -> f64 {
        f64::from(self.inner.text_width(&text, scale.unwrap_or(1)))
    }

    #[napi]
    pub fn text_height(scale: Option<u32>) -> f64 {
        f64::from(UiLight::text_height(scale.unwrap_or(1)))
    }

    /// Spread light above `threshold` into a glow of `strength`.
    #[napi]
    pub fn bloom(&mut self, threshold: f64, strength: f64) {
        self.inner.bloom(threshold as f32, strength as f32);
    }

    /// The buffer tone-mapped into an image.
    #[napi]
    pub fn to_image(&self) -> Image {
        Image::new(self.inner.to_surface())
    }
}

/// The keys held down right now.
#[napi]
pub struct Keys {
    inner: HeldKeys,
}

#[napi]
impl Keys {
    #[napi]
    pub fn is_held(&self, name: String) -> Result<bool> {
        Ok(self.inner.is_held(key(&name)?))
    }

    #[napi]
    pub fn any_held(&self, names: Vec<String>) -> Result<bool> {
        let keys = names
            .iter()
            .map(|name| key(name))
            .collect::<Result<Vec<Key>>>()?;
        Ok(self.inner.any_held(&keys))
    }
}

/// A point or direction in world space.
#[napi]
#[derive(Clone, Copy)]
#[must_use]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl From<UiV3> for V3 {
    fn from(at: UiV3) -> Self {
        Self {
            x: f64::from(at.x),
            y: f64::from(at.y),
            z: f64::from(at.z),
        }
    }
}

impl V3 {
    fn inner(self) -> UiV3 {
        v3(self.x as f32, self.y as f32, self.z as f32)
    }
}

/// A `V3`, or an `[x, y, z]` array.
type World<'a> = Either<&'a V3, (f64, f64, f64)>;

fn world(value: World<'_>) -> UiV3 {
    match value {
        Either::A(at) => at.inner(),
        Either::B((x, y, z)) => v3(x as f32, y as f32, z as f32),
    }
}

#[napi]
impl V3 {
    #[napi(constructor)]
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    #[napi(ts_args_type = "other: World")]
    pub fn add(&self, other: World<'_>) -> Self {
        (self.inner() + world(other)).into()
    }

    #[napi(ts_args_type = "other: World")]
    pub fn sub(&self, other: World<'_>) -> Self {
        (self.inner() - world(other)).into()
    }

    #[napi]
    pub fn mul(&self, scale: f64) -> Self {
        (self.inner() * scale as f32).into()
    }

    #[napi(ts_args_type = "other: World")]
    pub fn equals(&self, other: World<'_>) -> bool {
        world(other) == self.inner()
    }

    #[napi(ts_return_type = "[number, number, number]")]
    pub fn to_array(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }

    #[napi(ts_args_type = "other: World")]
    pub fn dot(&self, other: World<'_>) -> f64 {
        f64::from(self.inner().dot(world(other)))
    }

    #[napi(ts_args_type = "other: World")]
    pub fn cross(&self, other: World<'_>) -> Self {
        self.inner().cross(world(other)).into()
    }

    #[napi]
    pub fn normalized(&self) -> Self {
        self.inner().normalized().into()
    }

    #[napi]
    pub fn rotated_x(&self, angle: f64) -> Self {
        self.inner().rotated_x(angle as f32).into()
    }

    #[napi]
    pub fn rotated_y(&self, angle: f64) -> Self {
        self.inner().rotated_y(angle as f32).into()
    }

    #[napi]
    pub fn rotated_z(&self, angle: f64) -> Self {
        self.inner().rotated_z(angle as f32).into()
    }

    #[napi(js_name = "toString")]
    pub fn describe(&self) -> String {
        format!("V3({}, {}, {})", self.x, self.y, self.z)
    }
}

/// A pinhole camera looking down +z, with y up.
#[napi]
#[derive(Clone, Copy)]
pub struct Camera {
    center: Point,
    pub focal: f64,
    pub distance: f64,
    pub pixel_aspect: f64,
}

/// Where a point lands on screen and how many pixels one world unit spans there.
#[napi(object)]
pub struct Projection {
    #[napi(ts_type = "Point")]
    pub at: Point,
    pub scale: f64,
}

#[napi]
impl Camera {
    #[napi(
        constructor,
        ts_args_type = "center: Point, focal: number, distance: number, pixelAspect?: number"
    )]
    pub fn new(center: Point, focal: f64, distance: f64, pixel_aspect: Option<f64>) -> Self {
        Self {
            center,
            focal,
            distance,
            pixel_aspect: pixel_aspect.unwrap_or(1.0),
        }
    }

    #[napi(getter, ts_return_type = "Point")]
    pub fn center(&self) -> Point {
        self.center
    }

    #[napi(setter, js_name = "center", ts_args_type = "center: Point")]
    pub fn set_center(&mut self, center: Point) {
        self.center = center;
    }

    /// Where `at` lands on screen, or `null` behind the eye.
    #[napi(ts_args_type = "at: World")]
    pub fn project(&self, at: World<'_>) -> Option<Projection> {
        let camera = UiCamera {
            center: p2(self.center),
            focal: self.focal as f32,
            distance: self.distance as f32,
            pixel_aspect: self.pixel_aspect as f32,
        };
        camera
            .project(world(at))
            .map(|(on_screen, scale)| Projection {
                at: (f64::from(on_screen.x), f64::from(on_screen.y)),
                scale: f64::from(scale),
            })
    }
}

/// A small deterministic generator, so games replay the same in tests.
#[napi]
pub struct Rng {
    inner: UiRng,
}

#[napi]
impl Rng {
    #[napi(constructor)]
    pub fn new(seed: u32) -> Self {
        Self {
            inner: UiRng::new(seed),
        }
    }

    #[napi]
    pub fn next_u32(&mut self) -> u32 {
        self.inner.next_u32()
    }

    /// A value from `low` up to `high`.
    #[napi]
    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        f64::from(self.inner.range(low as f32, high as f32))
    }

    /// A random direction of unit length.
    #[napi]
    pub fn direction(&mut self) -> V3 {
        self.inner.direction().into()
    }
}

/// One glowing particle.
#[napi]
pub struct Spark {
    inner: UiSpark,
}

#[napi]
impl Spark {
    #[napi(
        constructor,
        ts_args_type = "at: World, velocity: World, color: LightColor, life: number, width?: number"
    )]
    pub fn new(
        at: World<'_>,
        velocity: World<'_>,
        color: Color,
        life: f64,
        width: Option<f64>,
    ) -> Self {
        Self {
            inner: UiSpark {
                at: world(at),
                velocity: world(velocity),
                color: rgb(color),
                age: 0.0,
                life: life as f32,
                width: width.unwrap_or(1.0) as f32,
            },
        }
    }
}

/// Every live spark, aged, moved and drawn together.
#[napi]
#[derive(Default)]
pub struct Sparks {
    inner: UiSparks,
}

#[napi]
impl Sparks {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[napi]
    pub fn add(&mut self, spark: &Spark) {
        self.inner.add(spark.inner);
    }

    #[napi(ts_args_type = "dt: number, gravity: World")]
    pub fn step(&mut self, dt: f64, gravity: World<'_>) {
        self.inner.step(dt as f32, world(gravity));
    }

    /// Draw every spark as a fading streak; `place(at)` gives its spot on
    /// screen, or `null` to skip it.
    #[napi(ts_args_type = "light: Light, place: (at: V3) => Point | null | undefined")]
    pub fn draw(&self, light: &mut Light, place: Function<'_, V3, Option<Point>>) -> Result<()> {
        let failed = RefCell::new(None);
        self.inner
            .draw(&mut light.inner, |at| match place.call(at.into()) {
                Ok(spot) => spot.map(p2),
                Err(error) => {
                    failed.borrow_mut().get_or_insert(error);
                    None
                }
            });
        failed.into_inner().map_or(Ok(()), Err)
    }

    #[napi(getter)]
    pub fn length(&self) -> u32 {
        u32::try_from(self.inner.len()).unwrap_or(u32::MAX)
    }

    #[napi]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// A JavaScript game, as the arcade loop sees it.
struct GameBridge {
    env: Env,
    game: Obj,
    light: Obj,
    keys: Obj,
    failure: Failure,
}

impl GameBridge {
    fn new(env: Env, game: Object<'_>, failure: Failure) -> Result<Self> {
        let light = Light::new(1, 1, None).into_instance(&env)?;
        let keys = Keys {
            inner: HeldKeys::default(),
        }
        .into_instance(&env)?;
        Ok(Self {
            env,
            game: hold(&env, game.to_unknown())?,
            light: hold(&env, light.as_object(&env).to_unknown())?,
            keys: hold(&env, keys.as_object(&env).to_unknown())?,
            failure,
        })
    }

    fn scoped<T>(&self, body: impl FnOnce(&Env, Object<'_>) -> Result<T>) -> Option<T> {
        let env = self.env;
        let outcome = env.run_in_scope(|| body(&env, object(&env, &self.game)?));
        outcome.map_err(|error| keep(&self.failure, error)).ok()
    }

    fn call_with_light(&self, name: &str, light: &mut UiLight) {
        self.scoped(|env, game| {
            let mut shared: ClassInstance<'_, Light> = fetch(env, &self.light)?;
            std::mem::swap(&mut shared.inner, light);
            let called = call_optional(&game, name, shared.as_object(env));
            std::mem::swap(&mut shared.inner, light);
            called.map(drop)
        });
    }

    fn failed(&self) -> bool {
        self.failure.borrow().is_some()
    }
}

impl UiGame for GameBridge {
    fn press(&mut self, key: Key) {
        self.scoped(|_, game| call_optional(&game, "press", key_name(key)).map(drop));
    }

    fn repeat(&mut self, key: Key) {
        self.scoped(|_, game| call_optional(&game, "repeat", key_name(key)).map(drop));
    }

    fn resize(&mut self, light: &UiLight) {
        let mut copy = light.clone();
        self.call_with_light("resize", &mut copy);
    }

    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        self.scoped(|env, game| {
            let mut shared: ClassInstance<'_, Keys> = fetch(env, &self.keys)?;
            shared.inner.clone_from(keys);
            let args = FnArgs::from((shared.as_object(env), f64::from(dt)));
            call_optional(&game, "step", args).map(drop)
        });
    }

    fn render(&mut self, light: &mut UiLight) {
        self.call_with_light("render", light);
    }

    fn finished(&self) -> bool {
        self.failed()
            || self
                .scoped(|_, game| match call_optional(&game, "finished", ())? {
                    Some(over) => over.coerce_to_bool(),
                    None => Ok(false),
                })
                .unwrap_or(true)
    }
}

/// Run `game` on the terminal until the player quits or it finishes. It
/// blocks until then, and an exception thrown inside the game reaches the
/// caller after the terminal is restored.
#[napi(ts_args_type = "game: GameLike")]
pub fn play(env: Env, game: Object<'_>) -> Result<()> {
    let failure = Failure::default();
    let bridge = GameBridge::new(env, game, Rc::clone(&failure))?;
    let finished = katui_game::play(bridge);
    if let Some(error) = take(&failure) {
        return Err(error);
    }
    finished.map_err(|error| terminal_error(&error))
}

/// How `frame` draws a game.
#[napi(object)]
#[derive(Default)]
pub struct FrameOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub held: Option<Vec<String>>,
    pub dt: Option<f64>,
}

/// `game` drawn off-terminal after `steps` steps with `held` keys down, for
/// tests and screenshots.
#[napi(ts_args_type = "game: GameLike, options?: FrameOptions")]
pub fn frame(env: Env, game: Object<'_>, options: Option<FrameOptions>) -> Result<Image> {
    let options = options.unwrap_or_default();
    let failure = Failure::default();
    let mut bridge = GameBridge::new(env, game, Rc::clone(&failure))?;
    let mut light = UiLight::new(
        options.width.unwrap_or(320),
        options.height.unwrap_or(180),
        1.0,
    );
    let mut keys = HeldKeys::default();
    let now = Instant::now();
    for name in options.held.unwrap_or_default() {
        keys.press(key(&name)?, now);
    }
    let dt = options.dt.map_or(1.0 / 60.0, |dt| dt as f32);
    bridge.resize(&light);
    for _ in 0..options.steps.unwrap_or(1) {
        bridge.step(&keys, dt);
    }
    bridge.render(&mut light);
    if let Some(error) = take(&failure) {
        return Err(error);
    }
    Ok(Image::new(light.to_surface()))
}
