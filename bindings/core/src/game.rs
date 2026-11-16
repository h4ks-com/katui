//! The bridge between katui's arcade loop and a game written in another
//! language, [`play`] and [`frame`].

use std::time::Instant;

use katui_game::{Game, HeldKeys, Light, P2};
use katui_proto::input::{InputEvent, Key, MouseEvent};
use katui_scene::surface::Surface;

use crate::Error;
use crate::event::key;
use crate::failure::Failure;

/// A game as a binding sees it: each hook calls into the host language and
/// reports a failure in the host's own error type.
pub trait GameHost {
    type Error;

    /// `error` raised the way the host raises its errors.
    fn refused(error: Error) -> Self::Error;

    fn press(&mut self, key: Key) -> Result<(), Self::Error>;

    fn repeat(&mut self, key: Key) -> Result<(), Self::Error>;

    /// `event`, a mouse event, happened at `at` in the light buffer's pixels.
    fn mouse(&mut self, event: &InputEvent, at: P2) -> Result<(), Self::Error>;

    /// The screen became the size of `light`, so baked art needs redoing.
    fn resize(&mut self, light: &mut Light) -> Result<(), Self::Error>;

    fn step(&mut self, keys: &HeldKeys, dt: f32) -> Result<(), Self::Error>;

    fn render(&mut self, light: &mut Light) -> Result<(), Self::Error>;

    /// Whether the next frame differs from the last one drawn.
    fn changed(&self) -> Result<bool, Self::Error>;

    fn finished(&self) -> Result<bool, Self::Error>;
}

/// A host game, as the arcade loop sees it: the first failure ends the game
/// and waits in its [`Failure`].
struct Played<H: GameHost> {
    host: H,
    failure: Failure<H::Error>,
}

impl<H: GameHost> Played<H> {
    fn kept(&self, done: Result<(), H::Error>) {
        if let Err(error) = done {
            self.failure.keep(error);
        }
    }
}

impl<H: GameHost> Game for Played<H> {
    fn press(&mut self, key: Key) {
        let done = self.host.press(key);
        self.kept(done);
    }

    fn repeat(&mut self, key: Key) {
        let done = self.host.repeat(key);
        self.kept(done);
    }

    fn mouse(&mut self, mouse: MouseEvent, at: P2) {
        let done = self.host.mouse(&InputEvent::Mouse(mouse), at);
        self.kept(done);
    }

    fn resize(&mut self, light: &Light) {
        let mut copy = light.clone();
        let done = self.host.resize(&mut copy);
        self.kept(done);
    }

    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        let done = self.host.step(keys, dt);
        self.kept(done);
    }

    fn render(&mut self, light: &mut Light) {
        let done = self.host.render(light);
        self.kept(done);
    }

    fn changed(&self) -> bool {
        self.failure.is_set()
            || self.host.changed().unwrap_or_else(|error| {
                self.failure.keep(error);
                true
            })
    }

    fn finished(&self) -> bool {
        self.failure.is_set()
            || self.host.finished().unwrap_or_else(|error| {
                self.failure.keep(error);
                true
            })
    }
}

/// Run `host` on the terminal until the player quits or it finishes. The
/// first failure of the host comes back after the terminal is restored.
pub fn play<H: GameHost>(host: H) -> Result<(), H::Error> {
    let failure = Failure::default();
    let played = Played {
        host,
        failure: failure.clone(),
    };
    let finished = katui_game::play(played);
    if let Some(error) = failure.take() {
        return Err(error);
    }
    finished.map_err(|error| H::refused(Error::Terminal(error.to_string())))
}

/// How [`frame`] draws a game.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub held: HeldKeys,
    pub dt: f32,
}

/// `host` drawn off-terminal after `frame.steps` steps with `frame.held`
/// keys down, for tests and screenshots.
pub fn frame<H: GameHost>(host: H, frame: &Frame) -> Result<Surface, H::Error> {
    let mut played = Played {
        host,
        failure: Failure::default(),
    };
    let mut light = Light::new(frame.width, frame.height, 1.0);
    played.resize(&light);
    for _ in 0..frame.steps {
        played.step(&frame.held, frame.dt);
    }
    played.render(&mut light);
    match played.failure.take() {
        Some(error) => Err(error),
        None => Ok(light.to_surface()),
    }
}

/// The keys named `names`, held down from now.
pub fn held<S: AsRef<str>>(names: &[S]) -> Result<HeldKeys, Error> {
    let mut keys = HeldKeys::default();
    let now = Instant::now();
    for name in names {
        keys.press(key(name.as_ref())?, now);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use katui_proto::input::{Mods, MouseAction};

    use super::*;

    /// Lights its whole buffer once it has stepped with the space bar held,
    /// or once clicked.
    struct Flash {
        lit: bool,
    }

    impl GameHost for Flash {
        type Error = String;

        fn refused(error: Error) -> String {
            error.to_string()
        }

        fn press(&mut self, _key: Key) -> Result<(), String> {
            Ok(())
        }

        fn repeat(&mut self, _key: Key) -> Result<(), String> {
            Ok(())
        }

        fn mouse(&mut self, event: &InputEvent, at: P2) -> Result<(), String> {
            self.lit = matches!(event, InputEvent::Mouse(_)) && at.x > 0.0;
            Ok(())
        }

        fn resize(&mut self, _light: &mut Light) -> Result<(), String> {
            Ok(())
        }

        fn step(&mut self, keys: &HeldKeys, _dt: f32) -> Result<(), String> {
            self.lit |= keys.is_held(Key::Char(' '));
            Ok(())
        }

        fn render(&mut self, light: &mut Light) -> Result<(), String> {
            if self.lit {
                light.paint(|_, _| [1.0; 3]);
                Ok(())
            } else {
                Err("hold space".to_owned())
            }
        }

        fn changed(&self) -> Result<bool, String> {
            if self.lit {
                Ok(true)
            } else {
                Err("unlit".to_owned())
            }
        }

        fn finished(&self) -> Result<bool, String> {
            Ok(false)
        }
    }

    #[test]
    fn a_click_reaches_the_host_and_a_failing_hook_ends_the_game() {
        let played = Played {
            host: Flash { lit: false },
            failure: Failure::default(),
        };
        assert!(played.changed(), "a failure draws the last frame");
        assert!(played.finished());
        let mut played = Played {
            host: Flash { lit: false },
            failure: Failure::default(),
        };
        let click = MouseEvent {
            action: MouseAction::Move,
            x: 0,
            y: 0,
            mods: Mods::NONE,
        };
        played.mouse(click, P2::new(3.0, 1.0));
        assert!(played.changed());
        assert!(!played.finished());
    }

    #[test]
    fn a_frame_steps_with_the_held_keys() -> Result<(), String> {
        let mut options = Frame {
            width: 4,
            height: 2,
            steps: 1,
            held: held(&[" "]).map_err(|error| error.to_string())?,
            dt: 1.0 / 60.0,
        };
        let surface = frame(Flash { lit: false }, &options)?;
        assert!(surface.pixels().first().is_some_and(|red| *red > 0));
        options.held = HeldKeys::default();
        assert_eq!(
            frame(Flash { lit: false }, &options).map(drop),
            Err("hold space".to_owned())
        );
        assert!(held(&["nope"]).is_err());
        Ok(())
    }
}
