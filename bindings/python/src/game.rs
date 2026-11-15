//! Arcade games: a Python `Game` drawn into a `Light` buffer at a fixed step.

use katui_bindings::failure::catching;
use katui_bindings::game::{Frame, GameHost, held};
use katui_bindings::names;
use katui_game::{
    BASE_IMAGE, Blend, Camera, HeldKeys, Light, MULTIPLEXED_SHRINK, P2, Rgb, Rng, Spark, Sparks,
    V3, v3,
};
use katui_proto::input::Key;
use pyo3::prelude::*;

use crate::app::call_optional;
use crate::convert::raise;
use crate::element::PyImage;
use crate::event::key;

type Color = (f32, f32, f32);
type Point = (f32, f32);

fn rgb((r, g, b): Color) -> Rgb {
    [r, g, b]
}

fn color_tuple([r, g, b]: Rgb) -> Color {
    (r, g, b)
}

fn p2((x, y): Point) -> P2 {
    P2::new(x, y)
}

fn blend(name: &str) -> PyResult<Blend> {
    names::blend(name).map_err(raise)
}

/// The light that shows as the sRGB color `red`, `green`, `blue`.
#[pyfunction]
pub(crate) fn srgb(red: u8, green: u8, blue: u8) -> Color {
    color_tuple(katui_game::srgb(red, green, blue))
}

/// `color` with every channel times `factor`.
#[pyfunction]
pub(crate) fn scaled(color: Color, factor: f32) -> Color {
    color_tuple(katui_game::scaled(rgb(color), factor))
}

/// The color `amount` of the way from `start` to `end`.
#[pyfunction]
pub(crate) fn lerp3(start: Color, end: Color, amount: f32) -> Color {
    color_tuple(katui_game::lerp3(rgb(start), rgb(end), amount))
}

/// A floating-point framebuffer games add light to.
#[pyclass(name = "Light", module = "katui.game")]
pub(crate) struct PyLight {
    inner: Light,
}

#[pymethods]
impl PyLight {
    #[new]
    #[pyo3(signature = (width, height, pixel_aspect=1.0))]
    fn new(width: u32, height: u32, pixel_aspect: f32) -> Self {
        Self {
            inner: Light::new(width, height, pixel_aspect),
        }
    }

    #[getter]
    fn width(&self) -> u32 {
        self.inner.width()
    }

    #[getter]
    fn height(&self) -> u32 {
        self.inner.height()
    }

    #[getter]
    fn pixel_aspect(&self) -> f32 {
        self.inner.pixel_aspect()
    }

    fn copy_from(&mut self, other: PyRef<'_, PyLight>) {
        self.inner.copy_from(&other.inner);
    }

    /// Fill every pixel with `shade(x, y)`, for backdrops baked once.
    fn paint(&mut self, shade: &Bound<'_, PyAny>) -> PyResult<()> {
        catching(|catch| {
            self.inner.paint(|x, y| {
                let shaded = shade.call1((x, y)).and_then(|made| made.extract::<Color>());
                catch.or(shaded.map(rgb), || [0.0; 3])
            });
        })
    }

    fn dim(&mut self, factor: f32) {
        self.inner.dim(factor);
    }

    #[pyo3(signature = (at, width, height, color, blend="over"))]
    fn rect(
        &mut self,
        at: Point,
        width: f32,
        height: f32,
        color: Color,
        blend: &str,
    ) -> PyResult<()> {
        self.inner
            .rect(p2(at), width, height, rgb(color), self::blend(blend)?);
        Ok(())
    }

    #[pyo3(signature = (points, color, blend="over"))]
    fn polygon(&mut self, points: Vec<Point>, color: Color, blend: &str) -> PyResult<()> {
        let points: Vec<P2> = points.into_iter().map(p2).collect();
        self.inner.polygon(&points, rgb(color), self::blend(blend)?);
        Ok(())
    }

    fn line(&mut self, start: Point, end: Point, width: f32, color: Color) {
        self.inner.line(p2(start), p2(end), width, rgb(color));
    }

    fn glow(&mut self, center: Point, radius: f32, color: Color) {
        self.inner.glow(p2(center), radius, rgb(color));
    }

    fn ring(&mut self, center: Point, radius: f32, width: f32, color: Color) {
        self.inner.ring(p2(center), radius, width, rgb(color));
    }

    #[pyo3(signature = (at, text, color, scale=1))]
    fn text(&mut self, at: Point, text: &str, color: Color, scale: u32) {
        self.inner.text(p2(at), text, scale, rgb(color));
    }

    #[pyo3(signature = (at, text, color, scale=1, blend="add"))]
    fn blended_text(
        &mut self,
        at: Point,
        text: &str,
        color: Color,
        scale: u32,
        blend: &str,
    ) -> PyResult<()> {
        self.inner
            .blended_text(p2(at), text, scale, rgb(color), self::blend(blend)?);
        Ok(())
    }

    #[pyo3(signature = (center_x, y, text, color, scale=1))]
    fn centered_text(&mut self, center_x: f32, y: f32, text: &str, color: Color, scale: u32) {
        self.inner
            .centered_text(center_x, y, text, scale, rgb(color));
    }

    #[pyo3(signature = (text, scale=1))]
    fn text_width(&self, text: &str, scale: u32) -> f32 {
        self.inner.text_width(text, scale)
    }

    #[staticmethod]
    #[pyo3(signature = (scale=1))]
    fn text_height(scale: u32) -> f32 {
        Light::text_height(scale)
    }

    /// Spread light above `threshold` into a glow of `strength`.
    fn bloom(&mut self, threshold: f32, strength: f32) {
        self.inner.bloom(threshold, strength);
    }

    /// The buffer tone-mapped into an image.
    fn to_image(&self) -> PyImage {
        PyImage::new(self.inner.to_surface())
    }
}

/// The keys held down right now.
#[pyclass(name = "Keys", module = "katui.game", frozen)]
pub(crate) struct PyKeys {
    inner: HeldKeys,
}

#[pymethods]
impl PyKeys {
    fn is_held(&self, name: &str) -> PyResult<bool> {
        Ok(self.inner.is_held(key(name)?))
    }

    fn any_held(&self, names: Vec<String>) -> PyResult<bool> {
        let keys = names
            .iter()
            .map(|name| key(name))
            .collect::<PyResult<Vec<Key>>>()?;
        Ok(self.inner.any_held(&keys))
    }
}

/// A point or direction in world space.
#[pyclass(name = "V3", module = "katui.game", frozen, get_all)]
#[derive(Clone, Copy)]
pub(crate) struct PyV3 {
    x: f32,
    y: f32,
    z: f32,
}

impl From<V3> for PyV3 {
    fn from(at: V3) -> Self {
        Self {
            x: at.x,
            y: at.y,
            z: at.z,
        }
    }
}

impl PyV3 {
    fn inner(self) -> V3 {
        v3(self.x, self.y, self.z)
    }
}

/// A `V3` from a `V3` or an `(x, y, z)` tuple.
fn world(value: &Bound<'_, PyAny>) -> PyResult<V3> {
    if let Ok(at) = value.cast::<PyV3>() {
        return Ok(at.get().inner());
    }
    let (x, y, z) = value.extract::<(f32, f32, f32)>()?;
    Ok(v3(x, y, z))
}

#[pymethods]
impl PyV3 {
    #[new]
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn __add__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok((self.inner() + world(other)?).into())
    }

    fn __sub__(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok((self.inner() - world(other)?).into())
    }

    fn __mul__(&self, scale: f32) -> Self {
        (self.inner() * scale).into()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        world(other).is_ok_and(|other| other == self.inner())
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let items = (self.x, self.y, self.z).into_pyobject(py)?;
        Ok(items.try_iter()?.into_any().unbind())
    }

    fn dot(&self, other: &Bound<'_, PyAny>) -> PyResult<f32> {
        Ok(self.inner().dot(world(other)?))
    }

    fn cross(&self, other: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(self.inner().cross(world(other)?).into())
    }

    fn normalized(&self) -> Self {
        self.inner().normalized().into()
    }

    fn rotated_x(&self, angle: f32) -> Self {
        self.inner().rotated_x(angle).into()
    }

    fn rotated_y(&self, angle: f32) -> Self {
        self.inner().rotated_y(angle).into()
    }

    fn rotated_z(&self, angle: f32) -> Self {
        self.inner().rotated_z(angle).into()
    }

    fn __repr__(&self) -> String {
        format!("V3({}, {}, {})", self.x, self.y, self.z)
    }
}

/// A pinhole camera looking down +z, with y up.
#[pyclass(name = "Camera", module = "katui.game", get_all, set_all)]
#[derive(Clone, Copy)]
pub(crate) struct PyCamera {
    center: Point,
    focal: f32,
    distance: f32,
    pixel_aspect: f32,
}

#[pymethods]
impl PyCamera {
    #[new]
    #[pyo3(signature = (center, focal, distance, pixel_aspect=1.0))]
    fn new(center: Point, focal: f32, distance: f32, pixel_aspect: f32) -> Self {
        Self {
            center,
            focal,
            distance,
            pixel_aspect,
        }
    }

    /// Where `at` lands on screen and how many pixels one world unit spans
    /// there, or `None` behind the eye.
    fn project(&self, at: &Bound<'_, PyAny>) -> PyResult<Option<(Point, f32)>> {
        let camera = Camera {
            center: p2(self.center),
            focal: self.focal,
            distance: self.distance,
            pixel_aspect: self.pixel_aspect,
        };
        Ok(camera
            .project(world(at)?)
            .map(|(on_screen, scale)| ((on_screen.x, on_screen.y), scale)))
    }
}

/// A small deterministic generator, so games replay the same in tests.
#[pyclass(name = "Rng", module = "katui.game")]
pub(crate) struct PyRng {
    inner: Rng,
}

#[pymethods]
impl PyRng {
    #[new]
    fn new(seed: u32) -> Self {
        Self {
            inner: Rng::new(seed),
        }
    }

    fn next_u32(&mut self) -> u32 {
        self.inner.next_u32()
    }

    /// A value from `low` up to `high`.
    fn range(&mut self, low: f32, high: f32) -> f32 {
        self.inner.range(low, high)
    }

    /// A random direction of unit length.
    fn direction(&mut self) -> PyV3 {
        self.inner.direction().into()
    }
}

/// One glowing particle.
#[pyclass(name = "Spark", module = "katui.game", frozen)]
#[derive(Clone, Copy)]
pub(crate) struct PySpark {
    inner: Spark,
}

#[pymethods]
impl PySpark {
    #[new]
    #[pyo3(signature = (at, velocity, color, life, width=1.0))]
    fn new(
        at: &Bound<'_, PyAny>,
        velocity: &Bound<'_, PyAny>,
        color: Color,
        life: f32,
        width: f32,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: Spark {
                at: world(at)?,
                velocity: world(velocity)?,
                color: rgb(color),
                age: 0.0,
                life,
                width,
            },
        })
    }
}

/// Every live spark, aged, moved and drawn together.
#[pyclass(name = "Sparks", module = "katui.game")]
#[derive(Default)]
pub(crate) struct PySparks {
    inner: Sparks,
}

#[pymethods]
impl PySparks {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    fn add(&mut self, spark: &PySpark) {
        self.inner.add(spark.inner);
    }

    fn step(&mut self, dt: f32, gravity: &Bound<'_, PyAny>) -> PyResult<()> {
        self.inner.step(dt, world(gravity)?);
        Ok(())
    }

    /// Draw every spark as a fading streak; `place(at)` gives its spot on
    /// screen, or `None` to skip it.
    fn draw(&self, light: &mut PyLight, place: &Bound<'_, PyAny>) -> PyResult<()> {
        catching(|catch| {
            self.inner.draw(&mut light.inner, |at| {
                let placed = place
                    .call1((PyV3::from(at),))
                    .and_then(|spot| spot.extract::<Option<Point>>());
                catch.or(placed, || None).map(p2)
            });
        })
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// A Python game, as the shared bridge calls it.
struct PyGame {
    game: Py<PyAny>,
    light: Py<PyLight>,
}

impl PyGame {
    fn new(py: Python<'_>, game: Py<PyAny>) -> PyResult<Self> {
        Ok(Self {
            game,
            light: Py::new(py, PyLight::new(1, 1, 1.0))?,
        })
    }

    fn call<A: for<'py> pyo3::call::PyCallArgs<'py>>(&self, name: &str, args: A) -> PyResult<()> {
        Python::attach(|py| call_optional(self.game.bind(py), name, args).map(drop))
    }

    /// Lend `light` to the game's `name` through our own `Light`.
    fn call_with_light(&self, name: &str, light: &mut Light) -> PyResult<()> {
        Python::attach(|py| {
            std::mem::swap(&mut self.light.borrow_mut(py).inner, light);
            let called = call_optional(self.game.bind(py), name, (self.light.clone_ref(py),));
            std::mem::swap(&mut self.light.borrow_mut(py).inner, light);
            called.map(drop)
        })
    }
}

impl GameHost for PyGame {
    type Error = PyErr;

    fn refused(error: katui_bindings::Error) -> PyErr {
        raise(error)
    }

    fn press(&mut self, key: Key) -> PyResult<()> {
        self.call("press", (key.name(),))
    }

    fn repeat(&mut self, key: Key) -> PyResult<()> {
        self.call("repeat", (key.name(),))
    }

    fn resize(&mut self, light: &mut Light) -> PyResult<()> {
        self.call_with_light("resize", light)
    }

    fn step(&mut self, keys: &HeldKeys, dt: f32) -> PyResult<()> {
        let keys = PyKeys {
            inner: keys.clone(),
        };
        self.call("step", (keys, dt))
    }

    fn render(&mut self, light: &mut Light) -> PyResult<()> {
        self.call_with_light("render", light)
    }

    fn finished(&self) -> PyResult<bool> {
        Python::attach(
            |py| match call_optional(self.game.bind(py), "finished", ())? {
                Some(over) => over.is_truthy(),
                None => Ok(false),
            },
        )
    }
}

/// Run `game` on the terminal until the player quits or it finishes.
#[pyfunction]
pub(crate) fn play(py: Python<'_>, game: Py<PyAny>) -> PyResult<()> {
    let host = PyGame::new(py, game)?;
    py.detach(|| katui_bindings::game::play(host))
}

/// `game` drawn off-terminal at `width` by `height` after `steps` steps of
/// `dt` seconds with `held` keys down, for tests and screenshots.
#[pyfunction]
#[pyo3(signature = (game, width=320, height=180, steps=1, held=Vec::new(), dt=1.0 / 60.0))]
pub(crate) fn frame(
    py: Python<'_>,
    game: Py<PyAny>,
    width: u32,
    height: u32,
    steps: u32,
    held: Vec<String>,
    dt: f32,
) -> PyResult<PyImage> {
    let frame = Frame {
        width,
        height,
        steps,
        held: self::held(&held).map_err(raise)?,
        dt,
    };
    katui_bindings::game::frame(PyGame::new(py, game)?, &frame).map(PyImage::new)
}

/// The first kitty image id games use, and how much smaller they draw
/// under a terminal multiplexer.
pub(crate) const GAME_CONSTANTS: [(&str, u32); 2] = [
    ("BASE_IMAGE", BASE_IMAGE),
    ("MULTIPLEXED_SHRINK", MULTIPLEXED_SHRINK),
];
