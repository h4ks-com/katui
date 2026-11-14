#![allow(clippy::cast_precision_loss)]

use std::io;
use std::time::{Duration, Instant};

use katui_present::select::{Backend, select};
use katui_present::session;
use katui_present::surface::{PatchedKitty, PixelScreen};
use katui_proto::input::{InputEvent, Key};
use katui_proto::probe::Capabilities;
use katui_rt::RtError;
use katui_rt::app::{App, AppEvent, Control, run_app};
use katui_rt::events::Waker;
use katui_scene::CellSize;

use crate::keys::HeldKeys;
use crate::light::Light;

const FRAMES_PER_SECOND: u32 = 60;
const STEP: Duration = Duration::from_micros(16_667);
const MOST_STEPS_PER_FRAME: u32 = 6;
/// The first kitty image id a full-screen pixel surface takes.
pub const BASE_IMAGE: u32 = 900;
const TILE_BUDGET: usize = 320;
const MOST_PIXELS_ACROSS: u32 = 1100;
const MOST_PIXELS_DOWN: u32 = 620;
/// How many times fewer pixels to send each way under a multiplexer, which
/// carries graphics far more slowly than a terminal does.
pub const MULTIPLEXED_SHRINK: u32 = 2;
/// We keep each height at twice its width so a tile of three heights spans whole cells.
const DENSITIES: [(u32, u32); 5] = [(10, 20), (8, 16), (5, 10), (4, 8), (2, 4)];
const TILE_HEIGHTS: u32 = 3;

/// A game [`play`] can run.
pub trait Game {
    /// `key` went down.
    fn press(&mut self, _key: Key) {}
    /// `key` auto-repeated while held.
    fn repeat(&mut self, _key: Key) {}
    /// The screen became the size of `light`, so baked art needs redoing.
    fn resize(&mut self, _light: &Light) {}
    /// Advance the world by `dt` seconds, a fixed step.
    fn step(&mut self, keys: &HeldKeys, dt: f32);
    fn render(&mut self, light: &mut Light);
}

/// Run `game` on the terminal until the player quits.
pub fn play<G: Game>(game: G) -> Result<(), RtError> {
    let caps = katui_rt::tty::probe_default();
    let mut arcade = Arcade::new(game, caps.clone());
    run_app(&mut arcade, io::stdout(), &caps, FRAMES_PER_SECOND)
}

struct Arcade<G> {
    game: G,
    caps: Capabilities,
    light: Light,
    screen: PixelScreen,
    fitted: Option<CellSize>,
    keys: HeldKeys,
    last_step: Instant,
    banked: Duration,
    asked_for_releases: bool,
}

fn density(size: CellSize, caps: &Capabilities) -> (u32, u32) {
    if select(caps) == Backend::Blocks {
        return (2, 2);
    }
    let shrink = if caps.multiplexed() {
        MULTIPLEXED_SHRINK
    } else {
        1
    };
    let fits = |(across, down): &(u32, u32)| {
        u32::from(size.columns) * across <= MOST_PIXELS_ACROSS / shrink
            && u32::from(size.rows) * down <= MOST_PIXELS_DOWN / shrink
    };
    DENSITIES.into_iter().find(fits).unwrap_or((2, 4))
}

impl<G: Game> Arcade<G> {
    fn new(game: G, caps: Capabilities) -> Self {
        Self {
            game,
            caps,
            light: Light::new(1, 1, 1.0),
            screen: PixelScreen::default(),
            fitted: None,
            keys: HeldKeys::default(),
            last_step: Instant::now(),
            banked: Duration::ZERO,
            asked_for_releases: false,
        }
    }

    fn fit(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<()> {
        if self.fitted == Some(size) {
            return Ok(());
        }
        let cell = katui_rt::tty::cell_size(&self.caps);
        let (across, down) = density(size, &self.caps);
        let pixel_width = f32::from(cell.width) / across as f32;
        let pixel_height = f32::from(cell.height) / down as f32;
        self.light = Light::new(
            u32::from(size.columns) * across,
            u32::from(size.rows) * down,
            pixel_width / pixel_height,
        );
        self.screen.free(out)?;
        self.screen = PixelScreen::new(
            &self.caps,
            cell,
            PatchedKitty::with_tiles(BASE_IMAGE, cell, down * TILE_HEIGHTS, TILE_BUDGET),
        );
        self.fitted = Some(size);
        self.game.resize(&self.light);
        Ok(())
    }

    fn advance(&mut self) {
        let now = Instant::now();
        self.banked += now.saturating_duration_since(self.last_step);
        self.last_step = now;
        self.keys.tick(now);
        let mut steps = 0;
        while self.banked >= STEP && steps < MOST_STEPS_PER_FRAME {
            self.game.step(&self.keys, STEP.as_secs_f32());
            self.banked -= STEP;
            steps += 1;
        }
        if steps == MOST_STEPS_PER_FRAME {
            self.banked = Duration::ZERO;
        }
    }

    fn on_input(&mut self, event: &InputEvent, now: Instant) -> Control {
        if event.is_quit_key() {
            return Control::Quit;
        }
        match event {
            InputEvent::Key(key, _) => {
                self.keys.tick(now);
                if self.keys.is_held(*key) {
                    self.game.repeat(*key);
                } else {
                    self.game.press(*key);
                }
                self.keys.press(*key, now);
            }
            InputEvent::KeyUp(key, _) => self.keys.release(*key),
            InputEvent::Focus(false) => self.keys.clear(),
            _ => {}
        }
        Control::Animate
    }
}

impl<G: Game> App for Arcade<G> {
    type Message = ();

    fn start(&mut self, _waker: Waker<()>) {}

    fn update(&mut self, event: AppEvent<()>) -> Control {
        match event {
            AppEvent::Input(input) => return self.on_input(&input, Instant::now()),
            AppEvent::Repaint => {
                self.keys.clear();
                self.last_step = Instant::now();
                self.screen.repaint();
            }
            AppEvent::Message(()) | AppEvent::Resized(_) => {}
        }
        Control::Animate
    }

    fn draw(&mut self, out: &mut Vec<u8>, size: CellSize) -> io::Result<Control> {
        if !self.asked_for_releases {
            self.asked_for_releases = true;
            session::report_key_releases(out)?;
        }
        self.fit(out, size)?;
        self.advance();
        self.game.render(&mut self.light);
        let surface = self.light.to_surface();
        self.screen
            .render_frame(out, &surface, size.columns, size.rows)?;
        Ok(Control::Animate)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use katui_proto::input::{InputEvent, Key, Mods};
    use katui_proto::probe::{Capabilities, CellPixels, Identity};
    use katui_rt::app::{App, Control};
    use katui_scene::CellSize;

    use super::{Arcade, Game, HeldKeys, Light, density};
    use crate::light::{Blend, P2};

    #[derive(Default)]
    struct Recorder {
        presses: u32,
        repeats: u32,
    }

    impl Game for Recorder {
        fn press(&mut self, _key: Key) {
            self.presses += 1;
        }
        fn repeat(&mut self, _key: Key) {
            self.repeats += 1;
        }
        fn resize(&mut self, _light: &Light) {}
        fn step(&mut self, _keys: &HeldKeys, _dt: f32) {}
        fn render(&mut self, light: &mut Light) {
            light.rect(P2::new(0.0, 0.0), 4.0, 4.0, [1.0; 3], Blend::Over);
        }
    }

    fn arcade() -> Arcade<Recorder> {
        Arcade::new(
            Recorder::default(),
            Capabilities {
                kitty: true,
                cell: Some(CellPixels {
                    width: 10,
                    height: 20,
                }),
                ..Capabilities::default()
            },
        )
    }

    #[test]
    fn the_first_frame_asks_for_releases_and_an_unchanged_one_writes_nothing() {
        let size = CellSize {
            columns: 40,
            rows: 12,
        };
        let mut arcade = arcade();
        let (mut first, mut second) = (Vec::new(), Vec::new());
        arcade.draw(&mut first, size).expect("a Vec accepts writes");
        arcade
            .draw(&mut second, size)
            .expect("a Vec accepts writes");
        let text = String::from_utf8_lossy(&first);
        assert!(text.contains("\x1b[=11;1u"));
        assert!(text.contains("\x1b_G"), "the first frame sends pixels");
        assert!(second.is_empty());
    }

    #[test]
    fn a_held_key_repeats_and_a_quit_key_quits() {
        let mut arcade = arcade();
        let now = Instant::now();
        let enter = InputEvent::Key(Key::Enter, Mods::NONE);
        arcade.on_input(&enter, now);
        arcade.on_input(&enter, now);
        assert_eq!((arcade.game.presses, arcade.game.repeats), (1, 1));
        let quit = InputEvent::Key(Key::Char('q'), Mods::NONE);
        assert_eq!(arcade.on_input(&quit, now), Control::Quit);
    }

    #[test]
    fn density_shrinks_to_stay_affordable() {
        let small = CellSize {
            columns: 80,
            rows: 24,
        };
        let huge = CellSize {
            columns: 274,
            rows: 72,
        };
        let graphics = Capabilities {
            kitty: true,
            ..Capabilities::default()
        };
        let multiplexed = Capabilities {
            identity: Some(Identity {
                name: "rmux".to_owned(),
                version: None,
            }),
            ..graphics.clone()
        };
        assert_eq!(density(small, &graphics), (10, 20));
        assert_eq!(density(huge, &graphics), (4, 8));
        assert_eq!(density(huge, &multiplexed), (2, 4));
        assert_eq!(density(huge, &Capabilities::default()), (2, 2));
    }
}
