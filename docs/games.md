# Games

This guide makes a small game: a glowing ball you steer with the arrow
keys, with a fading trail behind it. You learn how katui runs a game loop
and how to draw with light.

Add the `game` feature to your project:

```sh
cargo add katui --git https://github.com/matheusfillipe/katui --features game
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

Run it, hold the arrow keys, and press <kbd>Ctrl</kbd>+<kbd>C</kbd> to
quit.

## The loop

`play` owns the loop and calls your two methods:

- `step` moves the world forward by `dt` seconds. katui always calls it
  with the same `dt`, 60 times a second, however fast the terminal is. That
  keeps the game's speed and physics the same everywhere. When a frame
  takes longer, katui calls `step` several times before the next `render`.
- `render` paints the current world. katui calls it once per frame and
  sends only the parts of the picture that changed.

Three more methods are optional. `press` runs once when a key goes down,
`repeat` runs while the terminal repeats a held key, and `resize` runs when
the screen changes size, which is the place to redo anything you drew in
advance.

## Held keys

`HeldKeys` answers "is this key down right now?", which is what movement
needs. Terminals that support the kitty keyboard protocol report key
releases, so katui knows exactly when you let go. On other terminals katui
treats a key as held until its repeats stop.

Use `press` for things that happen once per key press, like firing or
pausing.

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
- `text`, `centered_text` and `text_width` draw with a built-in pixel
  font, in capital letters.
- `bloom(threshold, strength)` makes bright areas bleed into their
  surroundings.
- `paint` fills every pixel from a function, for backgrounds.

Coordinates are in game pixels, with the origin at the top left.
`light.width()` and `light.height()` tell you the size, which depends on
the terminal and its window. Game pixels can be taller than wide.
`light.pixel_aspect()` tells you by how much, and `glow` already accounts
for it, so a glow stays round.

## Next

The demos show bigger games built the same way. Run `just demo-shardrift`
for a space shooter and `just demo-lumenshaft` for a 3D block stacker, and
read their code in `crates/katui-game/examples/`.
