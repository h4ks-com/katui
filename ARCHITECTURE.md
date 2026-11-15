# katui architecture

The contributor contract: how the crates layer, where the seams are, and
what every test produces. [docs/architecture.md](docs/architecture.md)
explains the frame pipeline for readers.

## Rules

1. Terminal bytes come only from the katui-proto encoders, the
   katui-present presenters and the katui-rt setup and restore sequences.
   Every crate above hands its output to a presenter.
2. katui-proto, katui-scene and katui-present know nothing about widgets
   or layout.
3. Every crate except katui-rt performs zero I/O. Output goes through the
   `Sink` trait, the probe talks through the `Transport` trait, and input
   reaches an app as parsed events. `Env::capture` reading the process
   environment is the one exception.
4. Dependencies point strictly down the map below. katui-proto and
   katui-scene depend on nothing katui-internal and each define their own
   ids and rects; katui-present translates between them.

## Crate map

Each crate depends only on crates below it.

```
katui-c         the C ABI (bindings/c): app runner, widgets, html, games
                and a headless screen behind katui.h; bindings/go wraps it
                with cgo
katui-python    the katui Python package (bindings/python): app runner,
                widgets, html, games and a headless Screen over PyO3
katui-node      the katui npm package (bindings/node): the same surface over
                napi-rs, with generated TypeScript types
katui           every crate under one name; features: widgets, layout (default),
                draw, ui, app, game, content, html, ratatui
katui-ratatui   ratatui Backend adapter, image decode and fit, an event loop
                for ratatui apps behind the run feature
katui-app       run(app): pixel UI programs on the terminal
katui-game      pixel apps and games: light buffer, held keys, fixed-step runner
katui-content   markdown to scene cells; an html and css subset to ui elements
                behind the html feature
katui-widgets   theme, inputs, lists, tables, trees, scroll views, popups
katui-ui        reactive element trees, flexbox over taffy, hit testing, layers
katui-draw      canvas and text rasterizing onto surfaces
katui-layout    constraint splitting of cell rectangles
katui-rt        all I/O: /dev/tty, raw mode, signals, event pump, frame clock,
                pacing, the app loop and its headless driver
katui-present   presenters: cells, blocks, kitty placements, pixel surfaces
                (patched, animated, whole-frame, sixel, blocks)
katui-scene     cell buffers, RGBA surfaces with tile damage, placements
katui-proto     codecs (kitty, sixel, placeholders, rgp, clipboard), the input
                parser, the capability probe
```

## Seams

```rust
// katui-proto: the probe's channel to the terminal
pub trait Transport {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    fn read_timeout(&mut self, wait: Duration) -> Option<Vec<u8>>;
}
pub fn probe(transport: &mut dyn Transport, env: &Env) -> Capabilities;

// katui-present: where every presenter writes
pub trait Sink: std::io::Write {}
impl<T: std::io::Write> Sink for T {}

// katui-rt: an interactive application and the loop that drives it
pub trait App {
    type Message: Send + 'static;
    fn start(&mut self, waker: Waker<Self::Message>);
    fn update(&mut self, event: AppEvent<Self::Message>) -> Control;
    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control>;
}
pub fn run_app<A: App, W: io::Write>(
    app: &mut A, out: W, caps: &Capabilities, frames_per_second: u32,
) -> Result<(), RtError>;

// katui-rt: the same loop over fed input, with no terminal
impl<A: App> Headless<A> {
    pub fn start(app: A, size: CellSize) -> Self;
    pub fn type_bytes(&mut self, bytes: &[u8]);
    pub fn resize(&mut self, size: CellSize);
    pub fn frame(&mut self) -> io::Result<Option<Vec<u8>>>;
}
```

`TtyTransport` in katui-rt implements `Transport` over `/dev/tty`; tests
implement it over recorded replies.

## Tests

Tests run with `cargo test` and need no terminal.

- Encoders and presenters assert golden bytes.
- The input and reply parsers run seeded xorshift fuzz loops and must
  never panic.
- The probe replays recorded reply fixtures from real terminals
  (`crates/katui-proto/fixtures/probe/`).
- The raw-mode lifecycle runs against a real pty and skips where the
  sandbox denies device nodes.
- Apps run through `Headless`, which returns each frame's bytes.
- Draw, ui and widget tests render PNGs a human can look at.

Every test produces bytes, a scene or a PNG. If a test cannot produce one
of them, the module boundary is wrong.

## Dependencies

Runtime: base64 and flate2 (codecs), unicode-width and
unicode-segmentation (cell widths), rustix and signal-hook (katui-rt),
tiny-skia and cosmic-text (raster and text), taffy (flexbox), image
(decode), ratatui (the adapter), pulldown-cmark (markdown), scraper and
ego-tree (the html subset), thiserror (error enums). Every dependency is
declared once in `[workspace.dependencies]`. Copyleft stays out of the
build graph; cargo-deny enforces the license list.
