# katui architecture

The build skeleton. Grounding: research lives in `research/` (spec,
comparison, layers, plan, per-project dissections). This file turns that
into modules, interfaces, test strategy and priority order.

## Product

One Rust library: write a terminal app or game once, get crisp cells plus
real pixel graphics (kitty protocol primary), degrade gracefully everywhere
else (sixel, iTerm2, half/quarter-blocks), and stay testable headless.
v0.1 MVP: diffler runs on katui via the ratatui-compat backend, images via
the kitty placement layer, blocks fallback, snapshot test harness, and a
`katui-info` capability card binary (the notcurses pattern, doubles as the
probe integration test).

## Packaging and consumption modes

One workspace, one version line on crates.io, three ways to consume; the
same scene and presenters under all three:

1. As a ratatui backend, minimal: `cargo add katui-ratatui` and swap the
   backend generic in an existing ratatui app. Pulls only proto + present +
   scene core. Images and pixel planes appear under apps that never change
   their widget code (diffler is this mode).
2. As the generic engine: `cargo add katui` for the native scene API,
   runtime, widgets and draw crate. Features trim the tree: default =
   cells + kitty + blocks; `sixel`, `iterm2`, `canvas` (tiny-skia),
   `wgpu` (offscreen 3D), `docs` (markdown + tree-sitter), `html`
   (html5ever + selectors subset), `mcp` (control endpoint).
3. As a document/HTML renderer: `katui-content` with the `html` feature, the
   depth-2 subset from `research/html-css.md`, usable standalone or from
   ratatui apps through the adapter, in either fidelity mode.

## Prime directives

1. No layer above L1 emits bytes; presenters are the only byte producers.
2. No layer below L2 knows what a widget is.
3. The core (proto, scene, present, draw, layout, widgets, docs) performs
   zero I/O: everything flows through trait seams the pure crates define
   and katui-rt implements: `Transport` lives in katui-proto, `Sink` in
   katui-present, and `Source` (the event seam, still planned) will live
   in katui-rt. The probe's `Env::capture` reading the process
   environment is the one sanctioned exception. That single rule is what
   makes ~90 percent of the system unit-testable without a terminal.
4. Everything that talks to a real terminal is either an encoder test with
   golden bytes, or an integration tier concern (katui-rt + demo apps).

## Module map

```
katui-ratatui ─┐                        (compat adapter, spans L5..L1 from the side)
katui-ffi ──────┤
katui-game ─────┤  pixel apps and games: light buffer, held keys, fixed-step runner
katui-rt ───────┤  owns ALL I/O: raw mode, signals, event pump, frame clock,
                │  loops, MCP server, emergency restore
katui-content ──┤  markdown / tree-sitter / html-subset / charts -> scene
katui-ui ───────┤  reactive element trees, flexbox over taffy, hit testing -> layers
katui-widgets ──┤  theme, text field, focus, lists, controls -> ui elements
katui-layout ───┤  constraint API over taffy (+ kasuari for ratatui parity)
katui-draw ─────┤  text layout, tiny-skia canvas, sprites, markers
katui-scene ────┤  planes (cells + RGBA), damage, placements   [THE CONTRACT]
katui-present ──┤  Presenter::present(diff -> Sink): kitty F/P/A, sixel tiles,
                │  blocks, snapshot, later iterm2/browser/RGP
katui-proto ────┘  codecs + probes + Capabilities (pure)
```

Dependency direction: strictly downward. `katui-scene` and `katui-proto`
depend on nothing katui-internal and each define their own ids and rects,
protocol-neutral in the scene; katui-present translates between the two.

## The trait seams (the flexible interface)

```rust
// katui-proto
pub struct Capabilities { graphics: GraphicsCaps, keys: KeyCaps,
    color: ColorCaps, geometry: Geometry, terminal: Identity,
    transport: TransportKind, trust: ProbeSource }   // malevich trust model

pub trait Transport {                    // real: /dev/tty; tests: fixtures
    fn write(&mut self, bytes: &[u8]);
    fn read_timeout(&mut self, d: Duration) -> Option<Vec<u8>>;
}
pub fn probe(t: &mut dyn Transport, env: &EnvSniff) -> Capabilities;

pub mod kitty {   // command types with encode() methods, golden-byte tested
    impl Transmit<'_> { pub fn encode(&self) -> Vec<Vec<u8>>; }  // CHUNK-sized frames
    impl Place { pub fn encode(&self) -> Vec<u8>; }
    impl Delete { pub fn encode(&self) -> Vec<u8>; }
    pub fn parse_responses(bytes: &[u8]) -> Vec<Response>;  // total, never fails
}

// katui-scene: pure state, the hard contract
pub struct Scene { planes: PlaneList }              // z-ordered
pub enum Plane { Cells(CellPlane), Pixels(Surface) }
pub fn diff(prev: &Scene, next: &Scene) -> SceneDiff;
pub struct SceneDiff { cells: CellDeltas, surfaces: Vec<SurfaceDamage>,
                       placements: PlacementDelta }

// katui-present
pub trait Sink: std::io::Write {}
pub trait Presenter {
    fn present(&mut self, diff: &SceneDiff, caps: &Capabilities,
               out: &mut dyn Sink) -> FrameStats;
}
pub fn select(caps: &Capabilities) -> Backend;  // needs-aware choice lands with presenter strategies
pub struct Needs { alpha: bool, z_order: bool, fps: u8 }  // degrade ladder, lands with surfaces
pub struct FrameStats { bytes_out: usize, placements_moved: usize,
                        surfaces_patched: usize, encode_us: u64 }

// katui-rt: the only impure layer
pub trait Source { fn poll(&mut self, d: Duration) -> Option<Event>; }
pub struct Runtime { /* raw mode, loop, clock, keymaps, restore guards */ }
```

Interface stability: `katui-proto`, `katui-scene` and `Presenter` are
semver-frozen from v0.1; everything experimental hides behind feature flags
(the `instability` crate pattern ratatui uses).

## Per module: responsibility, unit tests, usability check

| module | responsibility | unit-testable core (tier 1) | usability check |
|---|---|---|---|
| katui-proto | bytes in/out of protocols, capability truth | encoders golden bytes (chunking m=1/m=0, 4096 boundary, multiple-of-4 rule; C=1 place; delete verbs); `parse_response` never panics under fuzz + interleaved keypress bytes (glkitty lesson); probe state machine against recorded reply fixtures from kitty/ghostty/wezterm/alacritty (opentui keeps such fixtures, we capture ours); geometry math | `katui-info` card in a real terminal |
| katui-scene | planes, diff, damage, placements | diff invariants: `diff(a,a)` empty, wide-glyph + VS16 trailing cells (fixtures ported from ratatui's test corpus), damage rect union, placement delta emits move-not-delete, create-before-delete ordering, content-hash suppression | scene→PNG dump eyeball in CI artifacts |
| katui-present | scene diff → sink bytes/ops | golden output per presenter for scripted diffs; strategy selection truth table (caps x transport x needs); blocks/quarter-block classifier on known pixel patterns (opentui algorithm); SSH budget pacing math; tmux DCS wrap of every chunk | blocks presenter renders test image in any terminal |
| katui-draw | raster + text + sprites | golden PNGs of canvas ops; wrap/width fixtures (urwid corpus); atlas packing invariants; marker glyph tables | canvas demo draws logos/shapes |
| katui-layout | constraint solving | geometry fixtures; taffy vs kasuari parity suite (same inputs, same rects) for the diffler layout corpus | port diffler's layouts unchanged |
| katui-widgets | components, focus, styling | render-widget-into-plane insta snapshots; focus graph routing; event propagation (bubble/capture); theme resolution incl. border sets (ttt model) | gallery app |
| katui-content | formats → scene (markdown, code, diff, html subset, charts) | markdown/tree-sitter/HTML-subset golden scene fixtures; both fidelity modes (html-css.md) | render diffler's markdown pane |
| katui-rt | ALL I/O and time | tier 2 only: headless driver over fake Sink/Source; loop deadline honored under synthetic load; keymap resolution tables; restore-guard determinism | raw-mode lifecycle across the terminal matrix |
| katui-ratatui | compat adapter | parity harness: render the ratatui demo widget corpus through katui, diff cell-for-cell against ratatui's own TestBackend expectations | diffler spike (the phase 1 gate) |

Testability tiers: tier 1 pure unit (no I/O, `cargo test`, insta, proptest
fuzz) covers proto, scene, present, draw, layout, widgets, docs. Tier 2
headless integration (fake Sink/Source in-process) covers rt and the
adapter. Tier 3 is the real-terminal matrix (kitty, ghostty, wezterm,
konsole, iTerm2, foot, WT), screenshot-based, run per release, never a
blocker for daily commits. Optional test oracle: a headless VT (avt) to
round-trip presenter bytes back into a grid and compare against the scene;
avt is below the popularity bar but asciinema-maintained and test-only.

## Dependencies (popular, active, permissive)

Runtime: tiny-skia (rasterizer, resvg-proven), taffy (flexbox+grid layout),
image (decode), flate2 + base64 (codecs, ubiquitous), serde + serde_json
(config, ops, MCP), ratatui (compat target), crossterm (raw mode and event
pumping in katui-rt initially; own termios path later if needed),
pulldown-cmark (markdown), tree-sitter + tree-sitter-highlight (code, the
diffler set), html5ever + servo `selectors` (HTML subset, phase 4),
cosmic-text (pixel-fidelity text, phase 2), wgpu behind a feature flag
(offscreen 3D, phase 2+), kasuari (via ratatui parity, small but it IS the
ratatui engine).

Tooling: insta (snapshots), proptest + cargo-fuzz (probe/parser fuzzing),
criterion (bench, byte budgets per presenter).

Later/bindings: napi-rs (TS), cbindgen (C ABI), wasm-bindgen (browser
presenter). Every runtime dep is MIT/Apache-2.0/Zlib class; anything
copyleft (chafa, ueberzugpp) stays an optional dynamic backend, never a
build dep.

## Build and test order (priority)

P0, all tier 1, no I/O, starts day one:

1. kitty codec + golden tests + response parser fuzz.
2. probe + Capabilities with captured reply fixtures.
3. scene cell plane + diff with ratatui-corpus fixtures.
4. blocks presenter + quarter classifier + round-trip through a headless
   grid; first PNG/PPM snapshot of a scene.
5. `select()` strategy table tests.

P1, adapter + first user:

6. katui-ratatui Backend impl + parity harness (tier 1, uses ratatui
   TestBackend expectations).
7. katui-rt headless driver + raw-mode lifecycle (tier 2).
8. image widget (async decode worker; diffler pipeline) + kitty placement
   presenter over real scenes.
9. diffler spike on a branch (tier 3 gate: kitty, ghostty, wezterm).

P2 and beyond follow research/plan.md phases 2-4: surfaces + Full/Patched/
Animation presenters, draw crate, then layout/widgets, then docs/bindings.

The sequencing rule: a module lands only with its tier 1 suite green and a
golden-output artifact a human can look at. Bytes, scenes and PNGs are the
three artifacts every test produces; if a test cannot emit one, the module
boundary is wrong.
