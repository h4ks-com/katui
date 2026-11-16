# Games

This guide makes a small game: a glowing ball you steer with the arrow
keys, with a fading trail behind it. You learn how katui runs a game loop
and how to draw with light.

Add the `game` feature to your project:

```sh
cargo add katui --features game
```

## The game

```rust,no_run
use katui::game::{Game, HeldKeys, Key, Light, P2, play, srgb};

struct Comet {
    at: P2,
}

impl Game for Comet {
    fn step(&mut self, keys: &HeldKeys, dt: f32) {
        let speed = 90.0 * dt;
        if keys.is_held(Key::Left) {
            self.at.x -= speed;
        }
        if keys.is_held(Key::Right) {
            self.at.x += speed;
        }
        if keys.is_held(Key::Up) {
            self.at.y -= speed;
        }
        if keys.is_held(Key::Down) {
            self.at.y += speed;
        }
    }

    fn render(&mut self, light: &mut Light) {
        light.dim(0.85);
        light.glow(self.at, 20.0, srgb(255, 140, 60));
        light.text(P2::new(4.0, 4.0), "arrows to steer", 1, srgb(120, 120, 140));
    }
}

fn main() -> Result<(), katui::rt::RtError> {
    play(Comet { at: P2::new(60.0, 40.0) })
}
```

Run it, hold the arrow keys, and press <kbd>q</kbd> or
<kbd>Ctrl</kbd>+<kbd>C</kbd> to quit. <kbd>Esc</kbd> reaches your game as
a key, so you can use it for a menu.

## The loop

`play` owns the loop and calls your two methods:

- `step` moves the world forward by `dt` seconds. katui always calls it
  with the same `dt`, 60 times a second, however fast the terminal is. That
  keeps the game's speed and physics the same everywhere. When a frame
  takes longer, katui calls `step` several times before the next `render`.
- `render` paints the current world. katui calls it once per frame and
  sends only the parts of the picture that changed.

The other methods are optional. `press` runs once when a key goes down,
`repeat` runs while the terminal repeats a held key, and `resize` runs when
the screen changes size, which is the place to redo anything you drew in
advance. `finished` ends the game when it returns true.

## Still frames

A board game or a menu sits still most of the time. Return false from
`changed` while nothing moves, and katui skips drawing and sending frames
and lets the CPU rest. It still steps your game ten times a second, so a
clock keeps running, and it calls `press` and `mouse` the moment input
arrives. Set a flag when something changes and clear it in `render`:

```rust,ignore
fn changed(&self) -> bool {
    self.dirty
}
```

## The mouse

`mouse` runs for every press, release, drag, move and wheel turn, with the
position in game pixels, the same coordinates you draw at:

```rust,ignore
fn mouse(&mut self, mouse: MouseEvent, at: P2) {
    if mouse.action == MouseAction::Press(Button::Left) {
        self.target = at;
    }
}
```

## Held keys

`HeldKeys` answers "is this key down right now?", which is what movement
needs. Terminals that support the kitty keyboard protocol report key
releases, so katui knows exactly when you let go. On other terminals katui
treats a key as held until its repeats stop.

Use `press` for things that happen once per key press, like firing or
pausing.

## Game controllers

With the `gamepad` feature, a game controller plays as keys: the d-pad and
the left stick are the arrow keys, the bottom face button is space, and the
right face button and start are enter. Your game needs no change for it.
On Linux the feature needs libudev (`libudev-dev` on Debian and Ubuntu).

```sh
cargo add katui --features game,gamepad
```

`katui::rt::gamepad::watch` gives you every controller in the standard
gamepad layout, with analog sticks and triggers, when you want more than
keys.

## Drawing with light

`Light` is the game's canvas. Its pixels hold light, not paint: two lights
in the same place add up, and values above 1.0 are brighter than white and
spill into a glow. That is why the comet's trail works with one line,
`light.dim(0.85)`: each frame keeps 85 percent of the last one, so old
positions fade out.

- `srgb(r, g, b)` converts an ordinary color to light.
- `glow`, `ring`, `line`, `rect` and `polygon` draw shapes. `rect` and
  `polygon` take a `Blend`: `Over` for solid things, `Add` for things that
  shine.
- `text`, `centered_text` and `text_width` draw with a built-in 5 by 7
  pixel font, for the retro look.
- `ui_text` and `ui_text_width` draw smooth text in katui's UI font at any
  size, for a sharp score or menu.
- `image` draws a picture or sprite from an RGBA surface, blended by its
  alpha.
- `pixel` and `set_pixel` read and write single pixels.
- `bloom(threshold, strength)` makes bright areas bleed into their
  surroundings.
- `paint` fills every pixel from a function, for backgrounds.

Coordinates are in game pixels, with the origin at the top left.
`light.width()` and `light.height()` tell you the size, which depends on
the terminal and its window. katui gives a game one pixel per screen pixel
when it can afford to, and fewer on a very large window or under a
multiplexer. Game pixels can be taller than wide. `light.pixel_aspect()`
tells you by how much, and `glow` and `ui_text` already account for it.

## Next

The demos show bigger games built the same way. Run `just demo-shardrift`
for a space shooter and `just demo-lumenshaft` for a 3D block stacker, and
read their code in `crates/katui-game/examples/`.
